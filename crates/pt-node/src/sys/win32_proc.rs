//! Process creation by hand: CreateProcessW, or CreateProcessAsUserW with another user's token
//! (environment and working directory are this process's), always suspended (so the caller can
//! place it in a Job Object before its first instruction), windowless, with exactly the three
//! std handles given and nothing else inherited.
#![allow(unsafe_code, non_snake_case, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

use super::win32::wide;

#[repr(C)]
struct STARTUPINFOW {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    x_size: u32,
    y_size: u32,
    x_chars: u32,
    y_chars: u32,
    fill_attribute: u32,
    flags: u32,
    show_window: u16,
    reserved2_len: u16,
    reserved2: *mut u8,
    std_input: isize,
    std_output: isize,
    std_error: isize,
}

#[repr(C)]
struct STARTUPINFOEXW {
    si: STARTUPINFOW,
    attributes: *mut c_void,
}

#[repr(C)]
struct PROCESS_INFORMATION {
    process: isize,
    thread: isize,
    pid: u32,
    tid: u32,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<STARTUPINFOW>() == 104);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateProcessW(
        app: *const u16,
        cmd: *mut u16,
        process_attrs: *mut c_void,
        thread_attrs: *mut c_void,
        inherit: i32,
        flags: u32,
        env: *const c_void,
        cwd: *const u16,
        startup: *const STARTUPINFOW,
        info: *mut PROCESS_INFORMATION,
    ) -> i32;
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: u32,
        attribute: usize,
        value: *const c_void,
        size: usize,
        previous: *mut c_void,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
    fn DuplicateHandle(
        source_process: isize,
        source: isize,
        target_process: isize,
        target: *mut isize,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn GetCurrentProcess() -> isize;
    fn WaitForSingleObject(h: isize, ms: u32) -> u32;
    fn GetExitCodeProcess(h: isize, code: *mut u32) -> i32;
    fn ResumeThread(h: isize) -> u32;
    fn TerminateProcess(h: isize, code: u32) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn CreateProcessAsUserW(
        token: isize,
        app: *const u16,
        cmd: *mut u16,
        process_attrs: *mut c_void,
        thread_attrs: *mut c_void,
        inherit: i32,
        flags: u32,
        env: *const c_void,
        cwd: *const u16,
        startup: *const STARTUPINFOW,
        info: *mut PROCESS_INFORMATION,
    ) -> i32;
}

#[link(name = "wtsapi32")]
unsafe extern "system" {
    fn WTSQueryUserToken(session: u32, token: *mut isize) -> i32;
}

/// A primary access token, closed on drop.
pub struct Token(OwnedHandle);

fn owned(h: isize) -> OwnedHandle {
    // SAFETY: callers pass a valid handle that nothing else owns.
    unsafe { OwnedHandle::from_raw_handle(h as RawHandle) }
}

impl Token {
    /// The signed-in user's token for a session. Needs SeTcbPrivilege (LocalSystem).
    pub fn session_user(session: u32) -> io::Result<Token> {
        let mut h = 0isize;
        // SAFETY: `h` is a live out-parameter; a nonzero return gives a token we now own.
        if unsafe { WTSQueryUserToken(session, &mut h) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Token(owned(h)))
    }
}

/// What to start. The child gets exactly `stdin`, `stdout` and `stderr` as its handles.
pub struct Spawn<'a> {
    pub token: Option<&'a Token>,
    pub command_line: String,
    /// For example `winsta0\default`; `None` inherits this process's.
    pub desktop: Option<&'a str>,
    pub stdin: &'a dyn AsRawHandle,
    pub stdout: &'a dyn AsRawHandle,
    pub stderr: &'a dyn AsRawHandle,
}

/// A process created suspended; `resume` starts it.
pub struct Process {
    process: OwnedHandle,
    thread: Option<OwnedHandle>,
}

impl AsRawHandle for Process {
    fn as_raw_handle(&self) -> RawHandle {
        self.process.as_raw_handle()
    }
}

/// Inheritable copies of the child's handles, closed when dropped.
struct Inheritable(Vec<isize>);

impl Inheritable {
    fn dup(&mut self, h: &dyn AsRawHandle) -> io::Result<isize> {
        const DUPLICATE_SAME_ACCESS: u32 = 2;
        let mut out = 0isize;
        // SAFETY: the source handle is valid for the borrow; the pseudo-handle of the current
        // process needs no closing; `out` is a live out-parameter.
        let ok = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                h.as_raw_handle() as isize,
                GetCurrentProcess(),
                &mut out,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        self.0.push(out);
        Ok(out)
    }
}

impl Drop for Inheritable {
    fn drop(&mut self) {
        for &h in &self.0 {
            drop(owned(h));
        }
    }
}

/// A PROC_THREAD_ATTRIBUTE_LIST holding one handle list.
struct Attributes {
    buf: Vec<u64>,
    handles: Vec<isize>,
}

impl Attributes {
    fn handle_list(handles: Vec<isize>) -> io::Result<Box<Attributes>> {
        const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x0002_0002;
        let mut size = 0usize;
        // SAFETY: a null list only asks for the size (the call "fails" with that size set).
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size) };
        let mut a = Box::new(Attributes {
            buf: vec![0u64; size.div_ceil(8)],
            handles,
        });
        // SAFETY: `buf` is writable for at least `size` bytes, 8-byte aligned.
        if unsafe { InitializeProcThreadAttributeList(a.list(), 1, 0, &mut size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the list is initialised; the handle array lives in the same Box as the list,
        // so it outlives every use of the list, as the API requires.
        let ok = unsafe {
            UpdateProcThreadAttribute(
                a.list(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                a.handles.as_ptr().cast(),
                a.handles.len() * std::mem::size_of::<isize>(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let e = io::Error::last_os_error();
            // SAFETY: the list was initialised above and is deleted once.
            unsafe { DeleteProcThreadAttributeList(a.list()) };
            a.buf.clear();
            return Err(e);
        }
        Ok(a)
    }

    fn list(&mut self) -> *mut c_void {
        self.buf.as_mut_ptr().cast()
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            // SAFETY: the list was initialised and is deleted exactly once.
            unsafe { DeleteProcThreadAttributeList(self.list()) };
        }
    }
}

/// One argument quoted the way the MSVC runtime (and Rust's std) splits a command line.
pub fn quote_arg(arg: &str, out: &mut String) {
    if !out.is_empty() {
        out.push(' ');
    }
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        out.push_str(arg);
        return;
    }
    out.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            backslashes += 1;
        } else {
            if c == '"' {
                out.extend(std::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
        }
        out.push(c);
    }
    out.extend(std::iter::repeat_n('\\', backslashes));
    out.push('"');
}

pub fn spawn(s: &Spawn) -> io::Result<Process> {
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x0008_0000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const STARTF_USESTDHANDLES: u32 = 0x100;

    let mut inherit = Inheritable(Vec::new());
    let (i, o, e) = (
        inherit.dup(s.stdin)?,
        inherit.dup(s.stdout)?,
        inherit.dup(s.stderr)?,
    );
    let mut attrs = Attributes::handle_list(vec![i, o, e])?;
    let mut cmd = wide(&s.command_line);
    let mut desktop = s.desktop.map(wide);
    let si = STARTUPINFOEXW {
        si: STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
            reserved: std::ptr::null_mut(),
            desktop: desktop
                .as_mut()
                .map_or(std::ptr::null_mut(), |d| d.as_mut_ptr()),
            title: std::ptr::null_mut(),
            x: 0,
            y: 0,
            x_size: 0,
            y_size: 0,
            x_chars: 0,
            y_chars: 0,
            fill_attribute: 0,
            flags: STARTF_USESTDHANDLES,
            show_window: 0,
            reserved2_len: 0,
            reserved2: std::ptr::null_mut(),
            std_input: i,
            std_output: o,
            std_error: e,
        },
        attributes: attrs.list(),
    };
    let flags = CREATE_SUSPENDED | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT;
    let mut pi = PROCESS_INFORMATION {
        process: 0,
        thread: 0,
        pid: 0,
        tid: 0,
    };
    // SAFETY: every pointer is either null where the API allows it or points into a buffer
    // that lives until after the call: the writable command line, the NUL-terminated desktop
    // and the attribute list, whose handle list
    // names only the inheritable duplicates held by `inherit`.
    let ok = unsafe {
        match s.token {
            Some(t) => CreateProcessAsUserW(
                t.0.as_raw_handle() as isize,
                std::ptr::null(),
                cmd.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                flags,
                std::ptr::null(),
                std::ptr::null(),
                &si.si,
                &mut pi,
            ),
            None => CreateProcessW(
                std::ptr::null(),
                cmd.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                flags,
                std::ptr::null(),
                std::ptr::null(),
                &si.si,
                &mut pi,
            ),
        }
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Process {
        process: owned(pi.process),
        thread: Some(owned(pi.thread)),
    })
}

impl Process {
    pub fn resume(&mut self) -> io::Result<()> {
        let Some(t) = self.thread.take() else {
            return Ok(());
        };
        // SAFETY: `t` is the primary thread handle CreateProcess returned, owned here.
        if unsafe { ResumeThread(t.as_raw_handle() as isize) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn wait_ms(&self, ms: u32) -> io::Result<Option<u32>> {
        const WAIT_OBJECT_0: u32 = 0;
        const WAIT_TIMEOUT: u32 = 0x102;
        let h = self.process.as_raw_handle() as isize;
        // SAFETY: `h` is our process handle, valid while `self` lives.
        match unsafe { WaitForSingleObject(h, ms) } {
            WAIT_OBJECT_0 => {
                let mut code = 0u32;
                // SAFETY: as above; `code` is a live out-parameter.
                if unsafe { GetExitCodeProcess(h, &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(code))
            }
            WAIT_TIMEOUT => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn terminate(&self, code: u32) -> io::Result<()> {
        // SAFETY: our process handle, valid while `self` lives.
        if unsafe { TerminateProcess(self.process.as_raw_handle() as isize, code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
