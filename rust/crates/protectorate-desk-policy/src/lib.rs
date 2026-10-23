//! Pure desktop access policy and key parsing shared by node RPC handlers.
mod keys;
pub use keys::{parse_chord, Chord, Key};

use chrono::{DateTime, Local, Timelike, Utc};
use chrono_tz::Tz;
use std::{fmt, str::FromStr, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidWindow(String),
    InvalidTime(String),
    InvalidIdle,
    InvalidTimezone(String),
    InvalidChord(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWindow(w) => write!(f, "window {w:?}, want HH:MM-HH:MM"),
            Self::InvalidTime(t) => write!(f, "time {t:?}, want HH:MM between 00:00 and 23:59"),
            Self::InvalidIdle => write!(f, "idle must be positive"),
            Self::InvalidTimezone(z) => write!(f, "invalid time zone {z:?}"),
            Self::InvalidChord(c) => write!(f, "invalid key chord {c:?}"),
        }
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

fn parse_clock(s: &str) -> Result<u16> {
    let (hours, minutes) = s
        .trim()
        .split_once(':')
        .ok_or_else(|| Error::InvalidTime(s.into()))?;
    if hours.is_empty()
        || minutes.len() != 2
        || !hours.bytes().all(|b| b.is_ascii_digit())
        || !minutes.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Error::InvalidTime(s.into()));
    }
    let hours: u16 = hours.parse().map_err(|_| Error::InvalidTime(s.into()))?;
    let minutes: u16 = minutes.parse().map_err(|_| Error::InvalidTime(s.into()))?;
    if hours > 23 || minutes > 59 {
        return Err(Error::InvalidTime(s.into()));
    }
    Ok(hours * 60 + minutes)
}

pub fn parse_window(s: &str) -> Result<(u16, u16)> {
    let (start, end) = s
        .trim()
        .split_once('-')
        .ok_or_else(|| Error::InvalidWindow(s.into()))?;
    let (start, end) = (parse_clock(start)?, parse_clock(end)?);
    if start == end {
        return Err(Error::InvalidWindow(s.into()));
    }
    Ok((start, end))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub start: u16,
    pub end: u16,
    pub idle: Duration,
    pub timezone: Option<Tz>,
    pub lock_after: bool,
}
impl Policy {
    pub fn new(window: &str, idle: Duration, timezone: &str, lock_after: bool) -> Result<Self> {
        let (start, end) = parse_window(window)?;
        if idle.is_zero() {
            return Err(Error::InvalidIdle);
        }
        let timezone = if timezone.is_empty() {
            None
        } else {
            Some(Tz::from_str(timezone).map_err(|_| Error::InvalidTimezone(timezone.into()))?)
        };
        Ok(Self {
            start,
            end,
            idle,
            timezone,
            lock_after,
        })
    }
    pub fn in_window(&self, now: DateTime<Utc>) -> bool {
        let minute = match self.timezone {
            Some(zone) => {
                let local = now.with_timezone(&zone);
                (local.hour() * 60 + local.minute()) as u16
            }
            None => {
                let local = now.with_timezone(&Local);
                (local.hour() * 60 + local.minute()) as u16
            }
        };
        if self.start < self.end {
            minute >= self.start && minute < self.end
        } else {
            minute >= self.start || minute < self.end
        }
    }
    pub fn window(&self) -> String {
        format!(
            "{:02}:{:02}-{:02}:{:02}",
            self.start / 60,
            self.start % 60,
            self.end / 60,
            self.end % 60
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ready {
    Interactive,
    NotInteractive,
    Unsupported,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenState {
    Unlocked,
    Locked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Available,
    OutsideWindow,
    UserActive,
    NotInteractive,
    Locked,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub available: bool,
    pub reason: Reason,
    pub detail: String,
}
pub struct Inputs {
    pub now: DateTime<Utc>,
    pub last_input: DateTime<Utc>,
    pub ready: Ready,
    pub screen: ScreenState,
}

/// The access decision. A missing policy allows any interactive desktop.
pub fn evaluate(policy: Option<&Policy>, input: Inputs) -> Verdict {
    let denied = |reason, detail: String| Verdict {
        available: false,
        reason,
        detail,
    };
    match input.ready {
        Ready::Unsupported => {
            return denied(Reason::Unsupported, "desktop control unsupported".into())
        }
        Ready::NotInteractive => {
            return denied(Reason::NotInteractive, "not an interactive session".into())
        }
        Ready::Interactive => {}
    }
    let Some(policy) = policy else {
        if input.screen == ScreenState::Locked {
            return denied(
                Reason::Locked,
                "the screen is locked; agents cannot drive a locked desktop, unlock it".into(),
            );
        }
        return Verdict {
            available: true,
            reason: Reason::Available,
            detail: "no policy: always available".into(),
        };
    };
    if !policy.in_window(input.now) {
        return denied(
            Reason::OutsideWindow,
            format!("outside the window {}", policy.window()),
        );
    }
    let idle = input
        .now
        .signed_duration_since(input.last_input)
        .to_std()
        .unwrap_or(Duration::ZERO);
    if idle < policy.idle {
        return denied(
            Reason::UserActive,
            format!(
                "user active: physical input {}s ago, {}s of idle required",
                idle.as_secs(),
                policy.idle.as_secs()
            ),
        );
    }
    if input.screen == ScreenState::Locked {
        return denied(
            Reason::Locked,
            "the screen is locked; agents cannot drive a locked desktop, unlock it".into(),
        );
    }
    Verdict {
        available: true,
        reason: Reason::Available,
        detail: "away: inside the window and idle".into(),
    }
}
