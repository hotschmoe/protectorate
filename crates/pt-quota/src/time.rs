//! Wall-clock helpers: epoch milliseconds and RFC 3339 parsing.

pub fn now_ms() -> u64 {
    pt_api::EpochMs::now().0
}

/// Parses `2026-11-01T05:36:00Z`, with optional fractional seconds and a numeric offset.
/// Anything else, or a time before 1970, is `None`.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 20 || !b.is_ascii() {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = s.get(from..to)?;
        if !part.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    if b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, min, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    let mut frac_ms = 0i64;
    if b[i] == b'.' {
        let start = i + 1;
        i = start;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
        let digits = &s[start..i.min(start + 3)];
        frac_ms = digits.parse::<i64>().ok()? * 10i64.pow(3 - digits.len() as u32);
    }
    let offset_secs = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if i + 6 == b.len() && b[i + 3] == b':' => {
            let oh = num(i + 1, i + 3)?;
            let om = num(i + 4, i + 6)?;
            if oh > 23 || om > 59 {
                return None;
            }
            let o = oh * 3600 + om * 60;
            if *sign == b'+' { o } else { -o }
        }
        _ => return None,
    };
    let secs =
        days_from_civil(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec - offset_secs;
    u64::try_from(secs)
        .ok()?
        .checked_mul(1000)?
        .checked_add(frac_ms as u64)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix seconds to epoch ms; negative or overflowing values are `None`.
pub fn secs_to_ms(secs: i64) -> Option<u64> {
    u64::try_from(secs).ok()?.checked_mul(1000)
}

/// A duration as `42m`, `3h07m` or `9d04h`.
pub fn fmt_age(ms: u64) -> String {
    let mins = (ms / 1000 + 30) / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    if mins < 48 * 60 {
        return format!("{}h{:02}m", mins / 60, mins % 60);
    }
    let hours = mins / 60;
    format!("{}d{:02}h", hours / 24, hours % 24)
}
