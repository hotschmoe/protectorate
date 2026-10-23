// A minimal JSON reader/writer that matches Go's encoding/json closely enough
// to read and write the mailbox log byte for byte.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Str(String),
    Int(i64),
    Float(f64),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_obj(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Obj(v) => Some(v),
            _ => None,
        }
    }
}

pub fn encode_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn quoted_array(xs: &[String]) -> String {
    let mut out = String::from("[");
    for (i, x) in xs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_string(&mut out, x);
    }
    out.push(']');
    out
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() {
            match self.b[self.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        match self.peek() {
            Some(b) if b == c => {
                self.i += 1;
                Ok(())
            }
            Some(b) => Err(format!(
                "invalid character {} looking for beginning of value",
                b as char
            )),
            None => Err("unexpected end of JSON input".to_string()),
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.peek() {
            None => Err("unexpected end of JSON input".to_string()),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => {
                self.literal("true")?;
                Ok(Value::Bool(true))
            }
            Some(b'f') => {
                self.literal("false")?;
                Ok(Value::Bool(false))
            }
            Some(b'n') => {
                self.literal("null")?;
                Ok(Value::Null)
            }
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number(),
            Some(b) => Err(format!(
                "invalid character {} looking for beginning of value",
                b as char
            )),
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), String> {
        if self.b.len() >= self.i + word.len()
            && &self.b[self.i..self.i + word.len()] == word.as_bytes()
        {
            self.i += word.len();
            Ok(())
        } else {
            Err(format!(
                "invalid character {} looking for beginning of value",
                self.peek().unwrap_or(b'?') as char
            ))
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut out: Vec<(String, Value)> = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(out));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            self.expect(b':')?;
            let v = self.value()?;
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = v,
                None => out.push((key, v)),
            }
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(out));
                }
                _ => return Err("invalid character after object key:value pair".to_string()),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut out: Vec<Value> = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(out));
        }
        loop {
            out.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(out));
                }
                _ => return Err("invalid character after array element".to_string()),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut raw: Vec<u8> = Vec::new();
        loop {
            let b = match self.peek() {
                Some(b) => b,
                None => return Err("unexpected end of JSON input".to_string()),
            };
            self.i += 1;
            match b {
                b'"' => {
                    return String::from_utf8(raw)
                        .map_err(|_| "invalid UTF-8 in string".to_string())
                }
                b'\\' => {
                    let e = match self.peek() {
                        Some(e) => e,
                        None => return Err("unexpected end of JSON input".to_string()),
                    };
                    self.i += 1;
                    match e {
                        b'"' => raw.push(b'"'),
                        b'\\' => raw.push(b'\\'),
                        b'/' => raw.push(b'/'),
                        b'b' => raw.push(0x08),
                        b'f' => raw.push(0x0c),
                        b'n' => raw.push(b'\n'),
                        b'r' => raw.push(b'\r'),
                        b't' => raw.push(b'\t'),
                        b'u' => {
                            let c = self.hex4()?;
                            if (0xD800..0xDC00).contains(&c) {
                                self.expect(b'\\')?;
                                self.expect(b'u')?;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err("invalid unicode escape".to_string());
                                }
                                let cp = 0x10000 + ((c - 0xD800) << 10) + (lo - 0xDC00);
                                push_char(
                                    &mut raw,
                                    char::from_u32(cp).ok_or("invalid unicode escape")?,
                                );
                            } else {
                                push_char(&mut raw, char::from_u32(c).unwrap_or('\u{fffd}'));
                            }
                        }
                        _ => return Err("invalid character in string escape code".to_string()),
                    }
                }
                b if b < 0x20 => {
                    return Err(format!("invalid character {} in string literal", b as char));
                }
                _ => raw.push(b),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.b.len() {
            return Err("unexpected end of JSON input".to_string());
        }
        let mut v: u32 = 0;
        for k in 0..4 {
            let d = hex_digit(self.b[self.i + k]).ok_or("invalid unicode escape")?;
            v = v * 16 + d;
        }
        self.i += 4;
        Ok(v)
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.peek() == Some(b'-') || self.peek() == Some(b'+') {
            self.i += 1;
        }
        let mut float = false;
        let mut digits = 0;
        while let Some(b) = self.peek() {
            match b {
                b'0'..=b'9' => {
                    digits += 1;
                    self.i += 1;
                }
                b'.' | b'e' | b'E' => {
                    float = true;
                    self.i += 1;
                }
                b'+' | b'-' => self.i += 1,
                _ => break,
            }
        }
        if digits == 0 {
            return Err("invalid number".to_string());
        }
        let text = std::str::from_utf8(&self.b[start..self.i])
            .map_err(|_| "invalid number".to_string())?;
        if float {
            text.parse::<f64>()
                .map(Value::Float)
                .map_err(|_| "invalid number".to_string())
        } else {
            match text.parse::<i64>() {
                Ok(v) => Ok(Value::Int(v)),
                Err(_) => text
                    .parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| "invalid number".to_string()),
            }
        }
    }
}

fn hex_digit(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a') as u32 + 10),
        b'A'..=b'F' => Some((b - b'A') as u32 + 10),
        _ => None,
    }
}

fn push_char(raw: &mut Vec<u8>, c: char) {
    let mut buf = [0u8; 4];
    raw.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

pub fn parse(text: &[u8]) -> Result<Value, String> {
    let mut p = Parser { b: text, i: 0 };
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(format!(
            "invalid character {} after top-level value",
            p.b[p.i] as char
        ));
    }
    Ok(v)
}
