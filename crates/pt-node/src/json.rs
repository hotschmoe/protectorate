//! Minimal deterministic JSON writer. Replaced by pt-json's writer once it lands (REVISIT).

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn array<I: IntoIterator<Item = String>>(items: I) -> String {
    let items: Vec<String> = items.into_iter().collect();
    format!("[{}]", items.join(","))
}

/// Object builder; keys appear in call order.
pub struct Obj {
    buf: String,
}

impl Obj {
    pub fn new() -> Obj {
        Obj {
            buf: String::from("{"),
        }
    }

    fn key(&mut self, k: &str) {
        if self.buf.len() > 1 {
            self.buf.push(',');
        }
        self.buf.push_str(&escape(k));
        self.buf.push(':');
    }

    pub fn raw(mut self, k: &str, v: &str) -> Obj {
        self.key(k);
        self.buf.push_str(v);
        self
    }

    pub fn str(self, k: &str, v: &str) -> Obj {
        self.raw(k, &escape(v))
    }

    pub fn uint(self, k: &str, v: u64) -> Obj {
        self.raw(k, &v.to_string())
    }

    pub fn bool(self, k: &str, v: bool) -> Obj {
        self.raw(k, if v { "true" } else { "false" })
    }

    pub fn float(self, k: &str, v: f64) -> Obj {
        if v.is_finite() {
            self.raw(k, &format!("{v:.2}"))
        } else {
            self.raw(k, "null")
        }
    }

    pub fn opt_str(self, k: &str, v: Option<&str>) -> Obj {
        match v {
            Some(s) => self.str(k, s),
            None => self.raw(k, "null"),
        }
    }

    pub fn opt_uint(self, k: &str, v: Option<u64>) -> Obj {
        match v {
            Some(n) => self.uint(k, n),
            None => self.raw(k, "null"),
        }
    }

    pub fn opt_bool(self, k: &str, v: Option<bool>) -> Obj {
        match v {
            Some(b) => self.bool(k, b),
            None => self.raw(k, "null"),
        }
    }

    pub fn finish(mut self) -> String {
        self.buf.push('}');
        self.buf
    }
}

impl Default for Obj {
    fn default() -> Obj {
        Obj::new()
    }
}

/// Strings from this machine are bounded and stripped of control characters before they leave it.
pub fn clean(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        let c = if c.is_control() { ' ' } else { c };
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}
