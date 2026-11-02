use crate::headers::Headers;

/// A parsed request with its whole body.
#[derive(Debug)]
pub struct Request {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) path_len: usize,
    pub(crate) http10: bool,
    pub(crate) headers: Headers,
    pub(crate) body: Vec<u8>,
}

impl Request {
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The path as sent, without the query and without percent-decoding.
    pub fn path(&self) -> &str {
        &self.target[..self.path_len]
    }

    /// The raw query after `?`, or "".
    pub fn query(&self) -> &str {
        self.target.get(self.path_len + 1..).unwrap_or("")
    }

    /// The first `name=value` query pair with that name, both percent-decoded (`+` is a space).
    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query().split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(k) == name).then(|| percent_decode(v))
        })
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
