//! pt-json and serde_json must agree on accept/reject, and on the value when both accept.
//! Accepted divergences, both where serde_json is stricter:
//! - numbers whose magnitude overflows f64 ("number out of range"): RFC 8259 allows them and
//!   pt-json keeps their text;
//! - exactly 128 nested arrays and objects: serde_json's limit is 127, ours 128.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_json::Value;
use serde_json::Value as Serde;

fuzz_target!(|data: &[u8]| {
    match (
        pt_json::parse_bytes(data),
        serde_json::from_slice::<Serde>(data),
    ) {
        (Ok(ours), Ok(theirs)) => {
            assert!(same(&ours, &theirs), "values differ: {ours} vs {theirs}")
        }
        (Err(_), Err(_)) => {}
        (Ok(_), Err(e)) => assert!(
            (e.to_string().starts_with("number out of range") && overflow_at(data, &e))
                || (e.to_string().starts_with("recursion limit exceeded")
                    && raw_depth(data) == 128),
            "pt-json accepts, serde_json rejects: {e}"
        ),
        (Err(e), Ok(_)) => panic!("pt-json rejects, serde_json accepts: {e}"),
    }
});

fn same(a: &Value, b: &Serde) -> bool {
    match (a, b) {
        (Value::Null, Serde::Null) => true,
        (Value::Bool(x), Serde::Bool(y)) => x == y,
        (Value::String(x), Serde::String(y)) => x == y,
        (Value::Number(x), Serde::Number(y)) => {
            if let Some(u) = y.as_u64() {
                x.as_u64() == Some(u)
            } else if let Some(i) = y.as_i64() {
                x.as_i64() == Some(i)
            } else {
                x.as_f64() == y.as_f64()
            }
        }
        (Value::Array(x), Serde::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Serde::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => false,
    }
}

fn is_number_byte(b: u8) -> bool {
    matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
}

/// The input number at serde_json's error position overflows f64. The error position, not our
/// value, is the oracle: a later duplicate key may have replaced that number.
fn overflow_at(data: &[u8], e: &serde_json::Error) -> bool {
    let line_start: usize = data
        .split(|&b| b == b'\n')
        .take(e.line() - 1)
        .map(|l| l.len() + 1)
        .sum();
    let at = line_start + e.column();
    [at.saturating_sub(1), at.saturating_sub(2)]
        .into_iter()
        .any(|p| {
            if !data.get(p).copied().is_some_and(is_number_byte) {
                return false;
            }
            let mut a = p;
            while a > 0 && is_number_byte(data[a - 1]) {
                a -= 1;
            }
            let mut z = p + 1;
            while z < data.len() && is_number_byte(data[z]) {
                z += 1;
            }
            std::str::from_utf8(&data[a..z])
                .ok()
                .and_then(|t| t.parse::<f64>().ok())
                .is_some_and(f64::is_infinite)
        })
}

/// Deepest array/object nesting in the raw input, skipping strings.
fn raw_depth(data: &[u8]) -> usize {
    let (mut depth, mut max, mut in_str, mut esc) = (0usize, 0, false, false);
    for &b in data {
        if in_str {
            match (esc, b) {
                (true, _) => esc = false,
                (false, b'\\') => esc = true,
                (false, b'"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'[' | b'{' => {
                depth += 1;
                max = max.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}
