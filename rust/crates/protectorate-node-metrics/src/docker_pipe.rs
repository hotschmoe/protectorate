//! Bounded, overlapped client I/O for a local Docker Engine named pipe.
use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    os::windows::ffi::OsStrExt,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_PIPE_BUSY,
        ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
        WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, OPEN_EXISTING},
    System::{
        Pipes::{SetNamedPipeHandleState, WaitNamedPipeW, PIPE_READMODE_BYTE},
        Threading::{CreateEventW, WaitForSingleObject},
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
    },
};

const MAX_OPERATION: usize = 64 * 1024;
const CANCEL_GRACE: u32 = 100;
const MAX_IN_FLIGHT: usize = 16;
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

struct OperationSlot;

impl OperationSlot {
    fn acquire() -> io::Result<Self> {
        IN_FLIGHT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_IN_FLIGHT).then_some(current + 1)
            })
            .map_err(|_| io::Error::other("too many pending Docker pipe operations"))?;
        Ok(Self)
    }
}

impl Drop for OperationSlot {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
    }
}

pub struct Pipe {
    handle: HANDLE,
    deadline: Instant,
}

impl Pipe {
    pub fn connect(path: &str, timeout: Duration) -> io::Result<Self> {
        let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Docker timeout too large")
        })?;
        loop {
            // SAFETY: wide is NUL-terminated and all optional handles are null.
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                // Docker HTTP responses are byte streams even when the pipe
                // server created a message-type pipe.
                let mode = PIPE_READMODE_BYTE;
                // SAFETY: handle is live; mode points to a stable local value.
                if unsafe { SetNamedPipeHandleState(handle, &mode, ptr::null(), ptr::null()) } == 0
                {
                    let error = io::Error::last_os_error();
                    // SAFETY: failed setup never transfers handle ownership.
                    unsafe { CloseHandle(handle) };
                    return Err(error);
                }
                return Ok(Self { handle, deadline });
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_PIPE_BUSY as i32) {
                return Err(error);
            }
            let remaining = remaining_ms(deadline)?;
            // SAFETY: wide remains valid for this bounded wait.
            if unsafe { WaitNamedPipeW(wide.as_ptr(), remaining) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
    }

    fn transfer(&mut self, data: &mut [u8], writing: bool) -> io::Result<usize> {
        let length = data.len().min(MAX_OPERATION);
        if length == 0 {
            return Ok(0);
        }
        remaining_ms(self.deadline)?;
        let slot = OperationSlot::acquire()?;
        let mut operation = Box::new(Operation::new(length)?);
        if writing {
            operation.buffer.copy_from_slice(&data[..length]);
        }
        // SAFETY: operation is heap-allocated, so the OVERLAPPED and buffer
        // addresses remain stable until completion or deliberate retention.
        let started = unsafe {
            if writing {
                WriteFile(
                    self.handle,
                    operation.buffer.as_ptr(),
                    length as u32,
                    ptr::null_mut(),
                    &mut operation.overlapped,
                )
            } else {
                ReadFile(
                    self.handle,
                    operation.buffer.as_mut_ptr(),
                    length as u32,
                    ptr::null_mut(),
                    &mut operation.overlapped,
                )
            }
        };
        if started == 0 {
            let error = io::Error::last_os_error();
            if !writing && pipe_eof(&error) {
                return Ok(0);
            }
            if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                return Err(error);
            }
            let wait = remaining_ms(self.deadline)
                .map(|ms| {
                    // SAFETY: operation owns a live manual-reset event.
                    unsafe { WaitForSingleObject(operation.event, ms) }
                })
                .unwrap_or(WAIT_TIMEOUT);
            if wait != WAIT_OBJECT_0 {
                let error = if wait == WAIT_FAILED {
                    io::Error::last_os_error()
                } else {
                    io::Error::new(io::ErrorKind::TimedOut, "Docker named pipe I/O timed out")
                };
                // SAFETY: cancellation targets only this operation.
                unsafe { CancelIoEx(self.handle, &operation.overlapped) };
                // A cancelled operation may still be in flight. Retain its
                // buffer, OVERLAPPED and event if it outlives the short grace.
                // The pipe handle closes when this failed request is dropped.
                if unsafe { WaitForSingleObject(operation.event, CANCEL_GRACE) } != WAIT_OBJECT_0 {
                    std::mem::forget(operation);
                    std::mem::forget(slot);
                }
                return Err(error);
            }
        }
        let mut transferred = 0;
        // SAFETY: completion was immediate or the operation event signaled.
        if unsafe { GetOverlappedResult(self.handle, &operation.overlapped, &mut transferred, 0) }
            == 0
        {
            let error = io::Error::last_os_error();
            if !writing && pipe_eof(&error) {
                return Ok(0);
            }
            return Err(error);
        }
        let count = transferred as usize;
        if !writing {
            data[..count].copy_from_slice(&operation.buffer[..count]);
        }
        Ok(count)
    }
}

fn pipe_eof(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code)
            if code == ERROR_BROKEN_PIPE as i32
                || code == ERROR_NO_DATA as i32
                || code == ERROR_PIPE_NOT_CONNECTED as i32
    )
}

impl Read for Pipe {
    fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        self.transfer(data, false)
    }
}

impl Write for Pipe {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        // The overlapped operation owns a copy until it completes or is
        // cancelled, so the caller may release its slice after this returns.
        self.transfer(&mut data[..data.len().min(MAX_OPERATION)].to_vec(), true)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: handle was returned by CreateFileW and is owned by Pipe.
        unsafe { CloseHandle(self.handle) };
    }
}

struct Operation {
    overlapped: OVERLAPPED,
    event: HANDLE,
    buffer: Vec<u8>,
}

impl Operation {
    fn new(length: usize) -> io::Result<Self> {
        // SAFETY: null security attributes and name request a private event.
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let overlapped = OVERLAPPED {
            hEvent: event,
            ..OVERLAPPED::default()
        };
        Ok(Self {
            overlapped,
            event,
            buffer: vec![0; length],
        })
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        // SAFETY: event was returned by CreateEventW and is owned by Operation.
        unsafe { CloseHandle(self.event) };
    }
}

fn remaining_ms(deadline: Instant) -> io::Result<u32> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Docker named pipe deadline expired",
        ));
    }
    Ok(remaining.as_millis().clamp(1, u32::MAX as u128) as u32)
}
