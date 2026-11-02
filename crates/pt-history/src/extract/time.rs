/// Milliseconds since the epoch for `YYYY-MM-DDTHH:MM:SS[.frac][Z|+HH:MM|-HH:MM]`.
pub fn parse_time(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let d = b.get(r)?;
        if d.is_empty() || !d.iter().all(u8::is_ascii_digit) {
            return None;
        }
        d.iter()
            .try_fold(0i64, |acc, c| Some(acc * 10 + i64::from(c - b'0')))
    };
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    let mut ms = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            if i - start < 3 {
                ms = ms * 10 + i64::from(b[i] - b'0');
            }
            i += 1;
        }
        if i == start {
            return None;
        }
        for _ in (i - start)..3 {
            ms *= 10;
        }
    }
    let offset_min = match b.get(i) {
        None | Some(b'Z') | Some(b'z') => 0,
        Some(&sign @ (b'+' | b'-')) => {
            let oh = num(i + 1..i + 3)?;
            let om = match b.get(i + 3) {
                Some(b':') => num(i + 4..i + 6)?,
                _ => num(i + 3..i + 5)?,
            };
            let m = oh * 60 + om;
            if sign == b'+' { m } else { -m }
        }
        _ => return None,
    };
    // Days from civil (Howard Hinnant's algorithm).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + mi * 60 + sec - offset_min * 60;
    let total = secs.checked_mul(1000)?.checked_add(ms)?;
    u64::try_from(total).ok()
}
