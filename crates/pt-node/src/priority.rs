use std::io;

/// Drops a running job to idle priority: nice 19 and the idle I/O class on Linux (the process and
/// every descendant, thread by thread), IDLE_PRIORITY_CLASS on Windows (the process only; its Job
/// Object should carry the class for children, REVISIT).
pub fn lower_to_idle(pid: u32) -> io::Result<()> {
    crate::sys::lower_to_idle(pid)
}
