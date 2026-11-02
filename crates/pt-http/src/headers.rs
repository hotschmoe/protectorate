/// Header fields in arrival order. Lookups ignore ASCII case; a name may repeat.
#[derive(Debug, Clone, Default)]
pub struct Headers(pub(crate) Vec<(String, String)>);

impl Headers {
    /// The first value of `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.get_all(name).next()
    }

    pub fn get_all<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a str> {
        self.0
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(n, v)| (n.as_str(), v.as_str()))
    }

    /// True when a comma-separated list header such as Connection holds `token` (any case).
    pub fn has_token(&self, name: &str, token: &str) -> bool {
        self.get_all(name)
            .flat_map(|v| v.split(','))
            .any(|t| t.trim().eq_ignore_ascii_case(token))
    }

    /// Appends a field. A field whose name is not a token, or whose value holds CR, LF or NUL,
    /// is dropped, so a handler cannot split the response.
    pub fn add(&mut self, name: &str, value: &str) {
        if is_token(name) && !value.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0)) {
            self.0.push((name.to_string(), value.to_string()));
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.0.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    }
}

pub(crate) fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}
