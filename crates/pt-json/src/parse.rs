use crate::{Error, Map, Number, Value};

const MAX_DEPTH: usize = 128;

/// Parses one JSON value, strictly per RFC 8259. Whitespace may surround it; anything else
/// after it is an error.
pub fn parse(text: &str) -> Result<Value, Error> {
    let mut p = Parser {
        s: text,
        b: text.as_bytes(),
        i: 0,
        depth: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.i < p.b.len() {
        return Err(Error::at(p.i, "trailing characters"));
    }
    Ok(v)
}

/// [`parse`] after validating UTF-8.
pub fn parse_bytes(bytes: &[u8]) -> Result<Value, Error> {
    match std::str::from_utf8(bytes) {
        Ok(s) => parse(s),
        Err(e) => Err(Error::at(e.valid_up_to(), "invalid UTF-8")),
    }
}

/// Index of the first `"`, `\` or control character at or after `i`, else `b.len()`.
/// Eight bytes at a time; the lowest flagged byte of each SWAR test is always a real match.
pub(crate) fn scan(b: &[u8], mut i: usize) -> usize {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    while let Some(chunk) = b.get(i..i + 8) {
        let w = u64::from_le_bytes(chunk.try_into().unwrap());
        let quote = w ^ (LO * b'"' as u64);
        let slash = w ^ (LO * b'\\' as u64);
        let m = ((w.wrapping_sub(LO * 0x20) & !w)
            | (quote.wrapping_sub(LO) & !quote)
            | (slash.wrapping_sub(LO) & !slash))
            & HI;
        if m != 0 {
            return i + (m.trailing_zeros() / 8) as usize;
        }
        i += 8;
    }
    while let Some(&c) = b.get(i) {
        if c == b'"' || c == b'\\' || c < 0x20 {
            break;
        }
        i += 1;
    }
    i
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b' ' | b'\n' | b'\r' | b'\t') = self.peek() {
            self.i += 1;
        }
    }

    fn unexpected(&self, expected: &'static str) -> Error {
        if self.i >= self.b.len() {
            Error::at(self.i, "unexpected end of input")
        } else {
            Error::at(self.i, expected)
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        match self.peek() {
            Some(b'"') => self.string().map(Value::String),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.unexpected("expected value")),
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value, Error> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(Error::at(self.i, "expected value"))
        }
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(Error::at(self.i, "nesting deeper than 128"));
        }
        self.i += 1;
        self.skip_ws();
        Ok(())
    }

    fn array(&mut self) -> Result<Value, Error> {
        self.enter()?;
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            self.depth -= 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    self.skip_ws();
                }
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(self.unexpected("expected ',' or ']'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Array(items))
    }

    fn object(&mut self) -> Result<Value, Error> {
        self.enter()?;
        let mut entries = Vec::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            self.depth -= 1;
            return Ok(Value::Object(Map::new()));
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(self.unexpected("expected string key"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.unexpected("expected ':'"));
            }
            self.i += 1;
            self.skip_ws();
            let v = self.value()?;
            entries.push((key, v));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    self.skip_ws();
                }
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(self.unexpected("expected ',' or '}'")),
            }
        }
        self.depth -= 1;
        Ok(Value::Object(Map::from_parsed(entries)))
    }

    fn string(&mut self) -> Result<String, Error> {
        let open = self.i;
        let start = open + 1;
        let mut i = scan(self.b, start);
        if self.b.get(i) == Some(&b'"') {
            self.i = i + 1;
            return Ok(self.s[start..i].to_owned());
        }
        let mut out = String::with_capacity(i - start + 16);
        let mut run = start;
        loop {
            match self.b.get(i) {
                Some(b'"') => {
                    out.push_str(&self.s[run..i]);
                    self.i = i + 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    out.push_str(&self.s[run..i]);
                    i = self.escape(i, &mut out)?;
                    run = i;
                }
                Some(_) => return Err(Error::at(i, "control character in string")),
                None => return Err(Error::at(open, "unterminated string")),
            }
            i = scan(self.b, i);
        }
    }

    /// Decodes the escape at `i` (a backslash) into `out`; returns the index after it.
    fn escape(&self, i: usize, out: &mut String) -> Result<usize, Error> {
        let c = match self.b.get(i + 1) {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                let hi = self.hex4(i + 2)?;
                let (cp, end) = match hi {
                    0xD800..=0xDBFF => {
                        if self.b.get(i + 6..i + 8) != Some(b"\\u") {
                            return Err(Error::at(i, "lone surrogate"));
                        }
                        let lo = self.hex4(i + 8)?;
                        if !(0xDC00..=0xDFFF).contains(&lo) {
                            return Err(Error::at(i, "lone surrogate"));
                        }
                        (0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00), i + 12)
                    }
                    0xDC00..=0xDFFF => return Err(Error::at(i, "lone surrogate")),
                    _ => (hi, i + 6),
                };
                let c = char::from_u32(cp).ok_or_else(|| Error::at(i, "invalid unicode escape"))?;
                out.push(c);
                return Ok(end);
            }
            Some(_) => return Err(Error::at(i, "invalid escape")),
            None => return Err(Error::at(i, "unterminated string")),
        };
        out.push(c);
        Ok(i + 2)
    }

    fn hex4(&self, at: usize) -> Result<u32, Error> {
        let mut n = 0;
        for k in at..at + 4 {
            let d = match self.b.get(k) {
                Some(&d @ b'0'..=b'9') => d - b'0',
                Some(&d @ b'a'..=b'f') => d - b'a' + 10,
                Some(&d @ b'A'..=b'F') => d - b'A' + 10,
                _ => return Err(Error::at(k, "invalid unicode escape")),
            };
            n = n * 16 + u32::from(d);
        }
        Ok(n)
    }

    fn number(&mut self) -> Result<Value, Error> {
        let b = self.b;
        let start = self.i;
        let mut i = start;
        if b[i] == b'-' {
            i += 1;
        }
        match b.get(i) {
            Some(b'0') => {
                i += 1;
                if matches!(b.get(i), Some(b'0'..=b'9')) {
                    return Err(Error::at(i - 1, "leading zero"));
                }
            }
            Some(b'1'..=b'9') => i = digits(b, i + 1),
            _ => return Err(Error::at(i, "invalid number")),
        }
        if b.get(i) == Some(&b'.') {
            let j = digits(b, i + 1);
            if j == i + 1 {
                return Err(Error::at(j, "invalid number"));
            }
            i = j;
        }
        if let Some(b'e' | b'E') = b.get(i) {
            i += 1;
            if let Some(b'+' | b'-') = b.get(i) {
                i += 1;
            }
            let j = digits(b, i);
            if j == i {
                return Err(Error::at(j, "invalid number"));
            }
            i = j;
        }
        self.i = i;
        Ok(Value::Number(Number::from_text(&self.s[start..i])))
    }
}

fn digits(b: &[u8], mut i: usize) -> usize {
    while let Some(b'0'..=b'9') = b.get(i) {
        i += 1;
    }
    i
}
