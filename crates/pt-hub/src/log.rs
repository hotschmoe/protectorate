//! Plain log lines to stderr: `<seconds since start> <level> <message>`.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);
static START: OnceLock<Instant> = OnceLock::new();

pub fn set_level(name: &str) -> Result<(), String> {
    let level = match name {
        "error" => Level::Error,
        "warn" => Level::Warn,
        "info" => Level::Info,
        "debug" => Level::Debug,
        _ => {
            return Err(format!(
                "unknown log level {name:?} (error, warn, info, debug)"
            ));
        }
    };
    LEVEL.store(level as u8, Ordering::Relaxed);
    Ok(())
}

pub fn enabled(level: Level) -> bool {
    level as u8 <= LEVEL.load(Ordering::Relaxed)
}

pub fn line(level: Level, args: std::fmt::Arguments<'_>) {
    let t = START.get_or_init(Instant::now).elapsed();
    let tag = match level {
        Level::Error => "error",
        Level::Warn => "warn",
        Level::Info => "info",
        Level::Debug => "debug",
    };
    eprintln!("{:>9.3} {tag} {args}", t.as_secs_f64());
}

macro_rules! log_at {
    ($level:expr, $($arg:tt)*) => {
        if $crate::log::enabled($level) {
            $crate::log::line($level, format_args!($($arg)*));
        }
    };
}

macro_rules! error {
    ($($arg:tt)*) => { $crate::log::log_at!($crate::log::Level::Error, $($arg)*) };
}
macro_rules! warning {
    ($($arg:tt)*) => { $crate::log::log_at!($crate::log::Level::Warn, $($arg)*) };
}
macro_rules! info {
    ($($arg:tt)*) => { $crate::log::log_at!($crate::log::Level::Info, $($arg)*) };
}
macro_rules! debug {
    ($($arg:tt)*) => { $crate::log::log_at!($crate::log::Level::Debug, $($arg)*) };
}

pub(crate) use {debug, error, info, log_at, warning};
