// Time in the shape Go's encoding/json writes it: RFC 3339 with nanoseconds,
// trailing zeros trimmed, always UTC.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Time {
    secs: i64,
    nanos: u32,
}

impl std::fmt::Debug for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format())
    }
}

impl Time {
    /// The value Go marshals a zero time.Time as.
    pub const GO_ZERO: Time = Time {
        secs: -62_135_596_800,
        nanos: 0,
    };

    pub fn now() -> Time {
        Self::from_system(SystemTime::now())
    }

    pub fn from_system(t: SystemTime) -> Time {
        match t.duration_since(UNIX_EPOCH) {
            Ok(d) => normalize(d.as_secs() as i64, d.subsec_nanos()),
            Err(e) => {
                let d = e.duration();
                let secs = -(d.as_secs() as i64);
                if d.subsec_nanos() == 0 {
                    Time { secs, nanos: 0 }
                } else {
                    normalize(secs - 1, 1_000_000_000 - d.subsec_nanos())
                }
            }
        }
    }

    pub fn to_system(self) -> SystemTime {
        if self.secs >= 0 {
            UNIX_EPOCH + Duration::new(self.secs as u64, self.nanos)
        } else {
            let back = Duration::new((-self.secs) as u64, 0) - Duration::new(0, self.nanos);
            UNIX_EPOCH - back
        }
    }

    pub fn from_secs_nanos(secs: i64, nanos: u32) -> Time {
        normalize(secs, nanos)
    }

    pub fn secs(self) -> i64 {
        self.secs
    }

    pub fn nanos(self) -> u32 {
        self.nanos
    }

    /// self - other, None when self is earlier than other.
    pub fn checked_duration_since(self, other: Time) -> Option<Duration> {
        let total = (self.secs as i128 - other.secs as i128) * 1_000_000_000
            + (self.nanos as i128 - other.nanos as i128);
        if total < 0 {
            return None;
        }
        let whole = total / 1_000_000_000;
        if whole > u64::MAX as i128 {
            return Some(Duration::MAX);
        }
        Some(Duration::new(whole as u64, (total % 1_000_000_000) as u32))
    }

    pub fn checked_add(self, d: Duration) -> Option<Time> {
        let secs = self.secs.checked_add(d.as_secs() as i64)?;
        Some(normalize(secs, self.nanos).add_nanos(d.subsec_nanos()))
    }

    pub fn format(self) -> String {
        let days = self.secs.div_euclid(SECONDS_PER_DAY);
        let sod = self.secs.rem_euclid(SECONDS_PER_DAY);
        let (y, m, d) = civil_from_days(days);
        let mut out = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            y,
            m,
            d,
            sod / 3600,
            (sod / 60) % 60,
            sod % 60
        );
        if self.nanos != 0 {
            let frac = format!("{:09}", self.nanos);
            out.push('.');
            out.push_str(frac.trim_end_matches('0'));
        }
        out.push('Z');
        out
    }

    pub fn parse(s: &str) -> Option<Time> {
        let bytes = s.as_bytes();
        if bytes.len() < 20 {
            return None;
        }
        let year = parse_int(&bytes[0..4], 4)?;
        if bytes[4] != b'-' {
            return None;
        }
        let month = parse_int(&bytes[5..7], 2)?;
        if bytes[7] != b'-' {
            return None;
        }
        let day = parse_int(&bytes[8..10], 2)?;
        if bytes[10] != b'T' && bytes[10] != b't' {
            return None;
        }
        let hour = parse_int(&bytes[11..13], 2)?;
        if bytes[13] != b':' {
            return None;
        }
        let minute = parse_int(&bytes[14..16], 2)?;
        if bytes[16] != b':' {
            return None;
        }
        let second = parse_int(&bytes[17..19], 2)?;
        let mut rest = &bytes[19..];
        let mut nanos = 0u32;
        if rest.first() == Some(&b'.') {
            rest = &rest[1..];
            let mut digits = 0;
            let mut value = 0u32;
            while let Some(&b) = rest.first() {
                if !b.is_ascii_digit() || digits == 9 {
                    break;
                }
                value = value * 10 + (b - b'0') as u32;
                digits += 1;
                rest = &rest[1..];
            }
            if digits == 0 {
                return None;
            }
            nanos = value * 10u32.pow(9 - digits);
        }
        let offset = if rest.first() == Some(&b'Z') || rest.first() == Some(&b'z') {
            if rest.len() != 1 {
                return None;
            }
            0
        } else {
            if rest.len() != 6 {
                return None;
            }
            let sign = match rest[0] {
                b'+' => 1i64,
                b'-' => -1i64,
                _ => return None,
            };
            if rest[3] != b':' {
                return None;
            }
            let oh = parse_int(&rest[1..3], 2)?;
            let om = parse_int(&rest[4..6], 2)?;
            sign * (oh * 3600 + om * 60)
        };
        if !(1..=12).contains(&month)
            || !(0..=31).contains(&day)
            || hour > 23
            || minute > 59
            || second > 60
        {
            return None;
        }
        let days = days_from_civil(year, month, day);
        let secs = days * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second - offset;
        Some(normalize(secs, nanos))
    }
}

fn parse_int(b: &[u8], len: usize) -> Option<i64> {
    if b.len() != len {
        return None;
    }
    let mut v = 0i64;
    for &c in b {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + (c - b'0') as i64;
    }
    Some(v)
}

fn normalize(secs: i64, nanos: u32) -> Time {
    if nanos >= 1_000_000_000 {
        return Time {
            secs: secs + (nanos as i64 / 1_000_000_000),
            nanos: nanos % 1_000_000_000,
        };
    }
    Time { secs, nanos }
}

impl Time {
    fn add_nanos(self, extra: u32) -> Time {
        let nanos = self.nanos as u64 + extra as u64;
        normalize(
            self.secs + (nanos / 1_000_000_000) as i64,
            (nanos % 1_000_000_000) as u32,
        )
    }
}

// Howard Hinnant's civil calendar algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let yy = if m <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
