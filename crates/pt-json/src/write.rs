use std::fmt::{self, Write};

use crate::Value;
use crate::parse::scan;

pub(crate) fn value<W: Write>(v: &Value, w: &mut W) -> fmt::Result {
    match v {
        Value::Null => w.write_str("null"),
        Value::Bool(true) => w.write_str("true"),
        Value::Bool(false) => w.write_str("false"),
        Value::Number(n) => w.write_str(n.as_str()),
        Value::String(s) => string(s, w),
        Value::Array(a) => {
            w.write_char('[')?;
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    w.write_char(',')?;
                }
                value(x, w)?;
            }
            w.write_char(']')
        }
        Value::Object(m) => {
            w.write_char('{')?;
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    w.write_char(',')?;
                }
                string(k, w)?;
                w.write_char(':')?;
                value(x, w)?;
            }
            w.write_char('}')
        }
    }
}

fn string<W: Write>(s: &str, w: &mut W) -> fmt::Result {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let b = s.as_bytes();
    w.write_char('"')?;
    let mut run = 0;
    let mut i = scan(b, 0);
    while i < b.len() {
        w.write_str(&s[run..i])?;
        match b[i] {
            b'"' => w.write_str("\\\"")?,
            b'\\' => w.write_str("\\\\")?,
            b'\n' => w.write_str("\\n")?,
            b'\t' => w.write_str("\\t")?,
            b'\r' => w.write_str("\\r")?,
            0x08 => w.write_str("\\b")?,
            0x0c => w.write_str("\\f")?,
            c => {
                w.write_str("\\u00")?;
                w.write_char(HEX[usize::from(c >> 4)] as char)?;
                w.write_char(HEX[usize::from(c & 15)] as char)?;
            }
        }
        run = i + 1;
        i = scan(b, run);
    }
    w.write_str(&s[run..])?;
    w.write_char('"')
}
