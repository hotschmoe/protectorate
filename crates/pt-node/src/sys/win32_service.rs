//! Running as a Windows service: the control dispatcher and handler, status reports, stdio to
//! a log file, and children placed in a kill-on-close job before their first instruction.
#![allow(unsafe_code, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::io::IntoRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};

use super::win32::{self, Job, wide};

#[repr(C)]
struct SERVICE_TABLE_ENTRYW {
    name: *mut u16,
    main: Option<unsafe extern "system" fn(u32, *mut *mut u16)>,
}

#[repr(C)]
struct SERVICE_STATUS {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
}

type Handler = extern "system" fn(u32, u32, *mut c_void, *mut c_void) -> u32;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn StartServiceCtrlDispatcherW(table: *const SERVICE_TABLE_ENTRYW) -> i32;
    fn RegisterServiceCtrlHandlerExW(name: *const u16, handler: Handler, ctx: *mut c_void)
    -> isize;
    fn SetServiceStatus(h: isize, status: *const SERVICE_STATUS) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetStdHandle(which: u32, h: isize) -> i32;
}

const SERVICE_WIN32_OWN_PROCESS: u32 = 0x10;
const STOPPED: u32 = 1;
const START_PENDING: u32 = 2;
const STOP_PENDING: u32 = 3;
const RUNNING: u32 = 4;
const ERROR_SERVICE_SPECIFIC_ERROR: u32 = 1066;
const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1063;

/// The service body: it calls the given function once it is up, then runs until the process
/// ends; returning means it failed or finished.
pub type Body = Box<dyn FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), String> + Send>;

enum Event {
    Stop,
    Exited(Result<(), String>),
}

static NAME: OnceLock<Vec<u16>> = OnceLock::new();
static BODY: Mutex<Option<Body>> = Mutex::new(None);
static EVENTS: Mutex<Option<mpsc::Sender<Event>>> = Mutex::new(None);
static OUTCOME: Mutex<Option<Result<(), String>>> = Mutex::new(None);
static STATUS: AtomicIsize = AtomicIsize::new(0);
static CHECKPOINT: AtomicU32 = AtomicU32::new(0);

fn report(state: u32, exit_code: u32, wait_hint: u32) {
    let h = STATUS.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    let pending = state == START_PENDING || state == STOP_PENDING;
    let st = SERVICE_STATUS {
        service_type: SERVICE_WIN32_OWN_PROCESS,
        current_state: state,
        // SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        controls_accepted: if state == RUNNING { 0x1 | 0x4 } else { 0 },
        win32_exit_code: if exit_code == 0 {
            0
        } else {
            ERROR_SERVICE_SPECIFIC_ERROR
        },
        service_exit_code: exit_code,
        check_point: if pending {
            CHECKPOINT.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            0
        },
        wait_hint,
    };
    // SAFETY: `h` came from RegisterServiceCtrlHandlerExW and stays valid for the process;
    // `st` is a live, fully initialised SERVICE_STATUS.
    unsafe { SetServiceStatus(h, &st) };
}

fn send(e: Event) {
    if let Some(tx) = EVENTS.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        let _ = tx.send(e);
    }
}

extern "system" fn handler(control: u32, _ty: u32, _data: *mut c_void, _ctx: *mut c_void) -> u32 {
    const STOP: u32 = 1;
    const INTERROGATE: u32 = 4;
    const SHUTDOWN: u32 = 5;
    match control {
        STOP | SHUTDOWN => {
            report(STOP_PENDING, 0, 10_000);
            send(Event::Stop);
            0
        }
        INTERROGATE => 0,
        _ => 120, // ERROR_CALL_NOT_IMPLEMENTED
    }
}

extern "system" fn service_main(_argc: u32, _argv: *mut *mut u16) {
    let Some(name) = NAME.get() else { return };
    // SAFETY: `name` is NUL-terminated and lives for the process (a static); `handler` has the
    // signature the API calls.
    let h = unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), handler, std::ptr::null_mut()) };
    if h == 0 {
        let e = io::Error::last_os_error();
        *OUTCOME.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(Err(format!("RegisterServiceCtrlHandlerExW: {e}")));
        return;
    }
    STATUS.store(h, Ordering::SeqCst);
    report(START_PENDING, 0, 30_000);
    let (tx, rx) = mpsc::channel();
    *EVENTS.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx.clone());
    let body = BODY.lock().unwrap_or_else(|p| p.into_inner()).take();
    std::thread::spawn(move || {
        let r = match body {
            Some(b) => b(Box::new(|| report(RUNNING, 0, 0))),
            None => Err("the service body ran twice".into()),
        };
        let _ = tx.send(Event::Exited(r));
    });
    let outcome = match rx.recv() {
        Ok(Event::Stop) | Err(_) => Ok(()),
        Ok(Event::Exited(r)) => r,
    };
    report(STOPPED, u32::from(outcome.is_err()), 0);
    *OUTCOME.lock().unwrap_or_else(|p| p.into_inner()) = Some(outcome);
}

/// Hands the process to the service control manager and runs `body` as the service named
/// `name`. Returns once the service has reported STOPPED; the caller then exits, which closes
/// every kill-on-close job.
pub fn run_service(name: &str, body: Body) -> Result<(), String> {
    if NAME.set(wide(name)).is_err() {
        return Err("run_service called twice".into());
    }
    *BODY.lock().unwrap_or_else(|p| p.into_inner()) = Some(body);
    let mut n = wide(name);
    let table = [
        SERVICE_TABLE_ENTRYW {
            name: n.as_mut_ptr(),
            main: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            name: std::ptr::null_mut(),
            main: None,
        },
    ];
    // SAFETY: `table` is a null-terminated array whose name buffer outlives the call, which
    // blocks until every service in it has stopped.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT) {
            return Err("--service runs only when the service manager starts it".into());
        }
        return Err(format!("StartServiceCtrlDispatcherW: {e}"));
    }
    OUTCOME
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
        .unwrap_or(Ok(()))
}

/// Points this process's stdout and stderr at `log` (a service has neither). std looks the
/// handles up at every write, so `eprintln!` lands there from now on.
pub fn redirect_stdio(log: File) -> io::Result<()> {
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    let h = log.into_raw_handle() as isize;
    for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: `h` is a valid file handle deliberately leaked for the process's lifetime.
        if unsafe { SetStdHandle(which, h) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Spawns `cmd` suspended and windowless, places it in `job` and only then lets it run.
pub fn spawn_in_job(cmd: &mut Command, job: &Job) -> io::Result<Child> {
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
    let mut child = cmd.spawn()?;
    if let Err(e) = job.assign(&child).and_then(|()| win32::resume(&child)) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::new(
            e.kind(),
            format!("placing the child in its job object: {e}"),
        ));
    }
    Ok(child)
}
