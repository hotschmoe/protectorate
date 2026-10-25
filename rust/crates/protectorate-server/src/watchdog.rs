//! Hang detection: a dedicated OS thread that exits the process when the
//! async runtime, the blocking pool or a state lock stops making progress, so
//! the service manager (systemd, Task Scheduler, runit) restarts it. Exiting
//! is the only recovery for a deadlock, and a hung process is invisible to
//! restart policies that only react to exits.
//!
//! Each probe runs on its own thread (or task) and stamps a timestamp after
//! every completed round. The watchdog thread only reads timestamps, so it can
//! never block on the lock it is watching, and it leaves with `_exit` so no
//! destructor or atexit handler can either.
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub const EXIT_CODE: i32 = 70;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(90);
const TIMEOUT_ENV: &str = "PROTECTORATE_WATCHDOG_TIMEOUT";

struct Entry {
    name: String,
    stamp: Arc<AtomicU64>,
}

pub struct Watchdog {
    role: &'static str,
    base: Instant,
    timeout: Duration,
    period: Duration,
    entries: Vec<Entry>,
}

pub struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn millis(base: Instant) -> u64 {
    base.elapsed().as_millis() as u64
}

impl Watchdog {
    /// Must run inside the tokio runtime it watches.
    pub fn new(
        role: &'static str,
        parse: impl Fn(&str) -> Result<Duration, String>,
    ) -> Result<Self, String> {
        let timeout = match std::env::var(TIMEOUT_ENV) {
            Ok(value) => parse(&value).map_err(|error| format!("{TIMEOUT_ENV}: {error}"))?,
            Err(_) => DEFAULT_TIMEOUT,
        };
        let period = (timeout / 10).clamp(Duration::from_millis(100), Duration::from_secs(5));
        let mut dog = Self {
            role,
            base: Instant::now(),
            timeout,
            period,
            entries: Vec::new(),
        };
        dog.async_probe("runtime heartbeat", || async {});
        dog.async_probe("blocking pool", || async {
            let _ = tokio::task::spawn_blocking(|| ()).await;
        });
        Ok(dog)
    }

    fn entry(&mut self, name: &str) -> Arc<AtomicU64> {
        let stamp = Arc::new(AtomicU64::new(millis(self.base)));
        self.entries.push(Entry {
            name: name.into(),
            stamp: Arc::clone(&stamp),
        });
        stamp
    }

    /// Runs `probe` forever on its own thread; it should take the lock it
    /// guards and return.
    pub fn sync_probe(&mut self, name: &str, probe: impl Fn() + Send + 'static) {
        let stamp = self.entry(name);
        let (base, period) = (self.base, self.period);
        let spawned = thread::Builder::new()
            .name(format!("pr-probe-{name}"))
            .spawn(move || loop {
                probe();
                stamp.store(millis(base), Ordering::Relaxed);
                thread::sleep(period);
            });
        if let Err(error) = spawned {
            eprintln!("watchdog: probe {name} not started: {error}");
        }
    }

    /// Runs `probe` forever as a task on the watched runtime.
    pub fn async_probe<F>(&mut self, name: &str, probe: impl Fn() -> F + Send + 'static)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let stamp = self.entry(name);
        let (base, period) = (self.base, self.period);
        tokio::spawn(async move {
            loop {
                probe().await;
                stamp.store(millis(base), Ordering::Relaxed);
                tokio::time::sleep(period).await;
            }
        });
    }

    /// Starts the watchdog thread. Dropping the guard disarms it, so a slow
    /// but orderly shutdown is not turned into a failure.
    pub fn arm(self) -> Result<Guard, String> {
        let disarmed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&disarmed);
        thread::Builder::new()
            .name("pr-watchdog".into())
            .spawn(move || self.run(&flag))
            .map_err(|error| format!("start watchdog thread: {error}"))?;
        Ok(Guard(disarmed))
    }

    fn run(self, disarmed: &AtomicBool) {
        let timeout = self.timeout.as_millis() as u64;
        // A SIGSTOP, a debugger or a frozen cgroup stops this thread too. A
        // late wake-up means the whole process was frozen, which is not a
        // hang: forgive everything before it.
        let frozen = (self.period * 3 + Duration::from_secs(1)).as_millis() as u64;
        let mut floor = 0;
        let mut last = millis(self.base);
        loop {
            thread::sleep(self.period);
            if disarmed.load(Ordering::Relaxed) {
                return;
            }
            let now = millis(self.base);
            if now - last > frozen {
                floor = now;
            }
            last = now;
            for entry in &self.entries {
                let age = now.saturating_sub(entry.stamp.load(Ordering::Relaxed).max(floor));
                if age > timeout {
                    self.fail(&entry.name, Duration::from_millis(age));
                }
            }
        }
    }

    fn fail(&self, name: &str, age: Duration) -> ! {
        let mut report = format!(
            "protectorate-rs {}: watchdog: {name} made no progress for {}s (limit {}s); exiting {EXIT_CODE} so the service manager restarts it\n",
            self.role,
            age.as_secs(),
            self.timeout.as_secs()
        );
        for other in &self.entries {
            let age = millis(self.base).saturating_sub(other.stamp.load(Ordering::Relaxed));
            report.push_str(&format!(
                "watchdog:   {}: last progress {age} ms ago\n",
                other.name
            ));
        }
        report.push_str(&threads());
        write_stderr(&report);
        terminate()
    }
}

#[cfg(target_os = "linux")]
fn threads() -> String {
    let mut out = String::new();
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return out;
    };
    for task in tasks.flatten() {
        let dir = task.path();
        let read = |file: &str| {
            std::fs::read_to_string(dir.join(file))
                .map(|text| text.trim().to_owned())
                .unwrap_or_default()
        };
        let stat = read("stat");
        let state = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.chars().next())
            .unwrap_or('?');
        out.push_str(&format!(
            "watchdog:   thread {} {} state {state} wchan {}\n",
            task.file_name().to_string_lossy(),
            read("comm"),
            read("wchan")
        ));
    }
    out
}

#[cfg(not(target_os = "linux"))]
fn threads() -> String {
    String::new()
}

#[cfg(unix)]
fn write_stderr(text: &str) {
    // Unlocked: another thread may hold the std stderr lock inside a write
    // that is itself stuck on a full pipe.
    let mut rest = text.as_bytes();
    while !rest.is_empty() {
        let written = unsafe { libc::write(2, rest.as_ptr().cast(), rest.len()) };
        if written <= 0 {
            return;
        }
        rest = &rest[written as usize..];
    }
}

#[cfg(not(unix))]
fn write_stderr(text: &str) {
    eprint!("{text}");
}

fn terminate() -> ! {
    #[cfg(unix)]
    unsafe {
        libc::_exit(EXIT_CODE)
    }
    #[cfg(not(unix))]
    std::process::exit(EXIT_CODE)
}
