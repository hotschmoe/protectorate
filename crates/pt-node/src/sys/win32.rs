//! Our own minimal Win32 declarations (no windows-sys). Every `unsafe` block sits in a safe
//! wrapper here, with the invariant it relies on.
#![allow(unsafe_code, non_snake_case, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::io;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FILETIME {
    low: u32,
    high: u32,
}

impl FILETIME {
    fn as_u64(self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}

#[repr(C)]
struct MEMORYSTATUSEX {
    length: u32,
    memory_load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}

#[repr(C)]
struct LASTINPUTINFO {
    size: u32,
    time: u32,
}

#[repr(C)]
struct WTS_SESSION_INFOW {
    session_id: u32,
    win_station_name: *mut u16,
    state: i32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(buf: *mut MEMORYSTATUSEX) -> i32;
    fn GetSystemTimes(idle: *mut FILETIME, kernel: *mut FILETIME, user: *mut FILETIME) -> i32;
    fn GetTickCount64() -> u64;
    fn GetTickCount() -> u32;
    fn GetDiskFreeSpaceExW(
        dir: *const u16,
        avail: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> i32;
    fn GetLogicalDriveStringsW(len: u32, buf: *mut u16) -> u32;
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetVolumeInformationW(
        root: *const u16,
        name: *mut u16,
        name_len: u32,
        serial: *mut u32,
        max_comp: *mut u32,
        flags: *mut u32,
        fs_name: *mut u16,
        fs_len: u32,
    ) -> i32;
    fn GetLogicalProcessorInformation(buf: *mut c_void, len: *mut u32) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn SetPriorityClass(process: isize, class: u32) -> i32;
    fn CloseHandle(h: isize) -> i32;
    fn WTSGetActiveConsoleSessionId() -> u32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
    fn GetCurrentProcessId() -> u32;
}

#[link(name = "wtsapi32")]
unsafe extern "system" {
    fn WTSEnumerateSessionsW(
        server: isize,
        reserved: u32,
        version: u32,
        out: *mut *mut WTS_SESSION_INFOW,
        count: *mut u32,
    ) -> i32;
    fn WTSQuerySessionInformationW(
        server: isize,
        session: u32,
        class: i32,
        out: *mut *mut u8,
        len: *mut u32,
    ) -> i32;
    fn WTSFreeMemory(p: *mut c_void);
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetLastInputInfo(info: *mut LASTINPUTINFO) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(key: isize, sub: *const u16, options: u32, sam: u32, out: *mut isize) -> i32;
    fn RegQueryValueExW(
        key: isize,
        name: *const u16,
        reserved: *mut u32,
        ty: *mut u32,
        data: *mut u8,
        len: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn utf16_le(b: &[u8]) -> Vec<u16> {
    b.as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect()
}

pub fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

pub struct Memory {
    pub total_phys: u64,
    pub avail_phys: u64,
    pub total_page: u64,
    pub avail_page: u64,
}

pub fn memory() -> Option<Memory> {
    let mut m = MEMORYSTATUSEX {
        length: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    // SAFETY: `m` is a live MEMORYSTATUSEX with `length` set as the API requires.
    let ok = unsafe { GlobalMemoryStatusEx(&mut m) };
    (ok != 0).then_some(Memory {
        total_phys: m.total_phys,
        avail_phys: m.avail_phys,
        total_page: m.total_page_file,
        avail_page: m.avail_page_file,
    })
}

/// (idle, kernel including idle, user) in 100 ns units.
pub fn system_times() -> Option<(u64, u64, u64)> {
    let (mut i, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: three live FILETIME out-parameters.
    let ok = unsafe { GetSystemTimes(&mut i, &mut k, &mut u) };
    (ok != 0).then(|| (i.as_u64(), k.as_u64(), u.as_u64()))
}

pub fn uptime_ms() -> u64 {
    // SAFETY: no arguments, no memory touched.
    unsafe { GetTickCount64() }
}

/// (available to caller, total, total free) bytes.
pub fn disk_space(root: &str) -> Option<(u64, u64, u64)> {
    let w = wide(root);
    let (mut a, mut t, mut f) = (0u64, 0u64, 0u64);
    // SAFETY: `w` is NUL-terminated and outlives the call; the out pointers are live u64s.
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut a, &mut t, &mut f) };
    (ok != 0).then_some((a, t, f))
}

pub fn logical_drives() -> Vec<String> {
    let mut buf = vec![0u16; 512];
    // SAFETY: `buf` is writable for the `len` elements passed.
    let n = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) } as usize;
    if n == 0 || n > buf.len() {
        return Vec::new();
    }
    buf[..n]
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

pub fn drive_type(root: &str) -> u32 {
    let w = wide(root);
    // SAFETY: `w` is NUL-terminated and outlives the call.
    unsafe { GetDriveTypeW(w.as_ptr()) }
}

pub fn volume_fs(root: &str) -> Option<String> {
    let w = wide(root);
    let mut fs = [0u16; 64];
    // SAFETY: `w` is NUL-terminated; `fs` is writable for 64 elements; the optional out pointers
    // are null, which the API allows.
    let ok = unsafe {
        GetVolumeInformationW(
            w.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    (ok != 0).then(|| from_wide(&fs))
}

/// Physical core count from SYSTEM_LOGICAL_PROCESSOR_INFORMATION records (RelationProcessorCore).
pub fn core_count() -> Option<u32> {
    let mut len = 0u32;
    // SAFETY: a null buffer with len 0 only asks for the required size.
    unsafe { GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len) };
    if len == 0 {
        return None;
    }
    // u64 elements give the 8-byte alignment of the records.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` is writable for at least `len` bytes.
    let ok = unsafe { GetLogicalProcessorInformation(buf.as_mut_ptr().cast(), &mut len) };
    if ok == 0 {
        return None;
    }
    const RECORD: usize = 32; // ULONG_PTR mask, DWORD relationship, 16-byte union (64-bit)
    let bytes = (len as usize).min(buf.len() * 8);
    let words = &buf[..bytes / 8];
    let mut cores = 0;
    for rec in words.as_chunks::<{ RECORD / 8 }>().0 {
        if rec[1] as u32 == 0 {
            cores += 1;
        }
    }
    Some(cores)
}

pub fn set_idle_priority(pid: u32) -> io::Result<()> {
    const PROCESS_SET_INFORMATION: u32 = 0x0200;
    const IDLE_PRIORITY_CLASS: u32 = 0x0040;
    // SAFETY: plain integer arguments; a zero return is checked below.
    let h = unsafe { OpenProcess(PROCESS_SET_INFORMATION, 0, pid) };
    if h == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `h` is a valid process handle we just opened.
    let ok = unsafe { SetPriorityClass(h, IDLE_PRIORITY_CLASS) };
    let result = if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    };
    // SAFETY: `h` is a valid handle owned here, closed exactly once.
    unsafe { CloseHandle(h) };
    result
}

/// `None` when no session is attached to the physical console.
pub fn console_session_id() -> Option<u32> {
    // SAFETY: no arguments, no memory touched.
    let id = unsafe { WTSGetActiveConsoleSessionId() };
    (id != 0xFFFF_FFFF).then_some(id)
}

pub fn own_session_id() -> Option<u32> {
    let mut s = 0u32;
    // SAFETY: `s` is a live out-parameter.
    let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut s) };
    (ok != 0).then_some(s)
}

pub struct WtsSession {
    pub id: u32,
    pub name: String,
    pub state: i32,
}

/// Reads a NUL-terminated UTF-16 string of at most 256 units; null gives an empty string.
fn wide_cstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut units = Vec::new();
    for i in 0..256 {
        // SAFETY: `p` is non-null and points into a WTS-owned NUL-terminated string; the read
        // stops at the terminator, so it never goes past the string.
        let c = unsafe { *p.add(i) };
        if c == 0 {
            break;
        }
        units.push(c);
    }
    String::from_utf16_lossy(&units)
}

pub fn wts_sessions() -> io::Result<Vec<WtsSession>> {
    let mut p: *mut WTS_SESSION_INFOW = std::ptr::null_mut();
    let mut n = 0u32;
    // SAFETY: out-pointers are live; on success `p` points to `n` records owned by WTS.
    let ok = unsafe { WTSEnumerateSessionsW(0, 0, 1, &mut p, &mut n) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: WTS returned `n` initialised records at `p`; they stay valid until WTSFreeMemory.
    let recs = unsafe { std::slice::from_raw_parts(p, n as usize) };
    let out = recs
        .iter()
        .map(|r| {
            let name = wide_cstr(r.win_station_name);
            WtsSession {
                id: r.session_id,
                name,
                state: r.state,
            }
        })
        .collect();
    // SAFETY: `p` came from WTSEnumerateSessionsW and is freed exactly once, after the last use.
    unsafe { WTSFreeMemory(p.cast()) };
    Ok(out)
}

pub struct SessionInfoEx {
    pub state: i32,
    pub flags: i32,
    pub user: String,
    pub last_input: i64,
    pub current: i64,
}

/// WTSINFOEXW (class 25), Level 1 layout on 64-bit: Level@0, SessionId@8, SessionState@12,
/// SessionFlags@16, WinStationName[33]@20, UserName[21]@86, DomainName[18]@128, LogonTime@168,
/// ConnectTime@176, DisconnectTime@184, LastInputTime@192, CurrentTime@200.
pub fn session_info_ex(session: u32) -> io::Result<SessionInfoEx> {
    const WTS_SESSION_INFO_EX: i32 = 25;
    let mut p: *mut u8 = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: out-pointers are live; on success `p` points to `len` bytes owned by WTS.
    let ok =
        unsafe { WTSQuerySessionInformationW(0, session, WTS_SESSION_INFO_EX, &mut p, &mut len) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: WTS returned `len` readable bytes at `p`; copied out before freeing.
    let raw = unsafe { std::slice::from_raw_parts(p, len as usize) }.to_vec();
    // SAFETY: `p` came from WTSQuerySessionInformationW and is freed exactly once.
    unsafe { WTSFreeMemory(p.cast()) };
    if raw.len() < 208 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("WTSINFOEXW is {} bytes, expected at least 208", raw.len()),
        ));
    }
    let u32_at = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap_or([0; 4]));
    let i64_at = |o: usize| i64::from_le_bytes(raw[o..o + 8].try_into().unwrap_or([0; 8]));
    let user = utf16_le(&raw[86..86 + 42]);
    if u32_at(0) != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("WTSINFOEXW level {} is not 1", u32_at(0)),
        ));
    }
    Ok(SessionInfoEx {
        state: u32_at(12) as i32,
        flags: u32_at(16) as i32,
        user: from_wide(&user),
        last_input: i64_at(192),
        current: i64_at(200),
    })
}

/// Milliseconds since the last input in the calling process's own session only.
pub fn own_session_idle_ms() -> Option<u32> {
    let mut info = LASTINPUTINFO {
        size: std::mem::size_of::<LASTINPUTINFO>() as u32,
        time: 0,
    };
    // SAFETY: `info` is live with `size` set as the API requires.
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if ok == 0 {
        return None;
    }
    // SAFETY: no arguments, no memory touched.
    let now = unsafe { GetTickCount() };
    Some(now.wrapping_sub(info.time))
}

const HKLM: isize = 0x8000_0002u32 as i32 as isize;
const KEY_READ_64: u32 = 0x0002_0019 | 0x0100;

/// Raw bytes and type of a HKLM registry value (64-bit view).
pub fn hklm_value(path: &str, name: &str) -> Option<(u32, Vec<u8>)> {
    let p = wide(path);
    let n = wide(name);
    let mut key = 0isize;
    // SAFETY: `p` is NUL-terminated; `key` is a live out-parameter.
    if unsafe { RegOpenKeyExW(HKLM, p.as_ptr(), 0, KEY_READ_64, &mut key) } != 0 {
        return None;
    }
    let (mut ty, mut len) = (0u32, 0u32);
    // SAFETY: a null data pointer only asks for type and size.
    let rc = unsafe {
        RegQueryValueExW(
            key,
            n.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            std::ptr::null_mut(),
            &mut len,
        )
    };
    let mut buf = vec![0u8; len as usize + 2];
    let mut cap = buf.len() as u32;
    let result = if rc == 0 {
        // SAFETY: `buf` is writable for `cap` bytes; `key` is open.
        let rc = unsafe {
            RegQueryValueExW(
                key,
                n.as_ptr(),
                std::ptr::null_mut(),
                &mut ty,
                buf.as_mut_ptr(),
                &mut cap,
            )
        };
        (rc == 0).then(|| {
            buf.truncate(cap as usize);
            (ty, buf)
        })
    } else {
        None
    };
    // SAFETY: `key` is a valid open key, closed exactly once.
    unsafe { RegCloseKey(key) };
    result
}

pub fn hklm_string(path: &str, name: &str) -> Option<String> {
    let (ty, bytes) = hklm_value(path, name)?;
    if ty != 1 && ty != 2 {
        return None;
    }
    Some(from_wide(&utf16_le(&bytes)))
}

pub fn hklm_uint(path: &str, name: &str) -> Option<u64> {
    let (_, b) = hklm_value(path, name)?;
    match b.len() {
        4 => Some(u64::from(u32::from_le_bytes(b[..4].try_into().ok()?))),
        8 => Some(u64::from_le_bytes(b[..8].try_into().ok()?)),
        _ => None,
    }
}

#[repr(C)]
#[derive(Default)]
struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
#[derive(Default)]
struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
    basic: JOBOBJECT_BASIC_LIMIT_INFORMATION,
    io_counters: [u64; 6],
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() == 144);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> isize;
    fn SetInformationJobObject(job: isize, class: i32, info: *const c_void, len: u32) -> i32;
    fn AssignProcessToJobObject(job: isize, process: isize) -> i32;
    fn TerminateJobObject(job: isize, exit_code: u32) -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: isize) -> i32;
}

fn process_handle(p: &impl std::os::windows::io::AsRawHandle) -> isize {
    p.as_raw_handle() as isize
}

/// A Job Object that kills every process in it when its last handle closes, including when the
/// agent dies.
pub struct Job(isize);

impl Job {
    pub fn new() -> io::Result<Job> {
        // SAFETY: null attributes and a null name are allowed (an unnamed, non-inheritable job).
        let h = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if h == 0 {
            return Err(io::Error::last_os_error());
        }
        let job = Job(h);
        job.set_limits(false)?;
        Ok(job)
    }

    fn set_limits(&self, idle: bool) -> io::Result<()> {
        const KILL_ON_JOB_CLOSE: u32 = 0x2000;
        const LIMIT_PRIORITY_CLASS: u32 = 0x20;
        const IDLE_PRIORITY_CLASS: u32 = 0x40;
        const EXTENDED_LIMIT_INFORMATION: i32 = 9;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.basic.limit_flags = KILL_ON_JOB_CLOSE;
        if idle {
            info.basic.limit_flags |= LIMIT_PRIORITY_CLASS;
            info.basic.priority_class = IDLE_PRIORITY_CLASS;
        }
        // SAFETY: `self.0` is a job handle we own; `info` is a live, fully initialised struct of
        // the size passed.
        let ok = unsafe {
            SetInformationJobObject(
                self.0,
                EXTENDED_LIMIT_INFORMATION,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn assign(&self, process: &impl std::os::windows::io::AsRawHandle) -> io::Result<()> {
        // SAFETY: both handles are valid: the job is ours, and `process` owns its process
        // handle for the duration of the borrow.
        let ok = unsafe { AssignProcessToJobObject(self.0, process_handle(process)) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Ends every process in the job with `code`.
    pub fn terminate(&self, code: u32) -> io::Result<()> {
        // SAFETY: `self.0` is a job handle we own.
        let ok = unsafe { TerminateJobObject(self.0, code) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// No access to other processes' USER handles, the clipboard, system parameters, display
    /// settings, global atoms, desktop switching or ExitWindows.
    pub fn restrict_ui(&self) -> io::Result<()> {
        const BASIC_UI_RESTRICTIONS: i32 = 4;
        const ALL_UI_LIMITS: u32 = 0xFF;
        // SAFETY: `self.0` is a job handle we own; the u32 is JOBOBJECT_BASIC_UI_RESTRICTIONS.
        let ok = unsafe {
            SetInformationJobObject(
                self.0,
                BASIC_UI_RESTRICTIONS,
                (&ALL_UI_LIMITS as *const u32).cast(),
                4,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// IDLE_PRIORITY_CLASS for every process in the job, running or started later.
    pub fn lower(&self) -> io::Result<()> {
        self.set_limits(true)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid handle owned here, closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Resumes a process created with CREATE_SUSPENDED.
pub fn resume(child: &std::process::Child) -> io::Result<()> {
    // SAFETY: `child` owns a process handle with full access for the duration of the borrow.
    let status = unsafe { NtResumeProcess(process_handle(child)) };
    if status < 0 {
        return Err(io::Error::other(format!(
            "NtResumeProcess failed with NTSTATUS {:#010x}",
            status as u32
        )));
    }
    Ok(())
}
