use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Index;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map),
}

static NULL: Value = Value::Null;

impl Value {
    /// The value at `key` if this is an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.as_object_mut()?.get_mut(key)
    }

    /// The element at `index` if this is an array.
    pub fn at(&self, index: usize) -> Option<&Value> {
        self.as_array()?.get(index)
    }

    pub fn at_mut(&mut self, index: usize) -> Option<&mut Value> {
        self.as_array_mut()?.get_mut(index)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Value::Number(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_number()?.as_i64()
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_number()?.as_u64()
    }

    pub fn as_f64(&self) -> Option<f64> {
        self.as_number()?.as_f64()
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Map> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    /// RFC 6901 lookup: `""` is the whole value, `"/a/0/b"` walks keys and indices, `~1` is
    /// `/` and `~0` is `~`.
    pub fn pointer(&self, pointer: &str) -> Option<&Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        let mut cur = self;
        for raw in pointer.strip_prefix('/')?.split('/') {
            let token = unescape_token(raw)?;
            cur = match cur {
                Value::Object(m) => m.get(&token)?,
                Value::Array(a) => a.get(array_index(&token)?)?,
                _ => return None,
            };
        }
        Some(cur)
    }

    pub fn pointer_mut(&mut self, pointer: &str) -> Option<&mut Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        let mut cur = self;
        for raw in pointer.strip_prefix('/')?.split('/') {
            let token = unescape_token(raw)?;
            cur = match cur {
                Value::Object(m) => m.get_mut(&token)?,
                Value::Array(a) => a.get_mut(array_index(&token)?)?,
                _ => return None,
            };
        }
        Some(cur)
    }

    /// Moves the value out, leaving `Null`.
    pub fn take(&mut self) -> Value {
        std::mem::take(self)
    }

    pub fn into_string(self) -> Option<String> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn into_array(self) -> Option<Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn into_object(self) -> Option<Map> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }
}

fn unescape_token(raw: &str) -> Option<Cow<'_, str>> {
    if !raw.contains('~') {
        return Some(Cow::Borrowed(raw));
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next()? {
                '0' => out.push('~'),
                '1' => out.push('/'),
                _ => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(Cow::Owned(out))
}

fn array_index(token: &str) -> Option<usize> {
    let b = token.as_bytes();
    if b.is_empty() || (b.len() > 1 && b[0] == b'0') || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    token.parse().ok()
}

/// `v["key"]` and `v[0]` give `Null` when the key, index or container is missing.
impl Index<&str> for Value {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }
}

impl Index<usize> for Value {
    type Output = Value;
    fn index(&self, index: usize) -> &Value {
        self.at(index).unwrap_or(&NULL)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::String(s.to_owned())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::String(s)
    }
}

impl From<Number> for Value {
    fn from(n: Number) -> Value {
        Value::Number(n)
    }
}

/// A non-finite `f64` becomes `Null`.
impl From<f64> for Value {
    fn from(f: f64) -> Value {
        Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

impl From<Vec<Value>> for Value {
    fn from(a: Vec<Value>) -> Value {
        Value::Array(a)
    }
}

impl From<Map> for Value {
    fn from(m: Map) -> Value {
        Value::Object(m)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(o: Option<T>) -> Value {
        o.map_or(Value::Null, Into::into)
    }
}

/// A JSON number as its exact text. Parsed numbers keep the source spelling; constructed ones
/// use Rust's shortest round-trip form.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Number(Box<str>);

impl Number {
    pub(crate) fn from_text(text: &str) -> Number {
        Number(text.into())
    }

    /// `None` for NaN and infinities.
    pub fn from_f64(f: f64) -> Option<Number> {
        f.is_finite().then(|| Number(format!("{f:?}").into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The value if the text is an integer (no fraction or exponent) that fits.
    pub fn as_i64(&self) -> Option<i64> {
        self.0.parse().ok()
    }

    pub fn as_u64(&self) -> Option<u64> {
        if &*self.0 == "-0" {
            return Some(0);
        }
        self.0.parse().ok()
    }

    /// The nearest `f64`, or `None` if the magnitude overflows to infinity.
    pub fn as_f64(&self) -> Option<f64> {
        self.0.parse::<f64>().ok().filter(|f| f.is_finite())
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

macro_rules! from_int {
    ($($t:ty)*) => {$(
        impl From<$t> for Number {
            fn from(n: $t) -> Number {
                Number(n.to_string().into())
            }
        }
        impl From<$t> for Value {
            fn from(n: $t) -> Value {
                Value::Number(Number::from(n))
            }
        }
    )*};
}

from_int!(i8 i16 i32 i64 isize u8 u16 u32 u64 usize);

/// An insertion-ordered object with unique keys. Lookups are linear, which beats hashing for
/// the small objects JSON APIs carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    entries: Vec<(String, Value)>,
}

impl Map {
    pub fn new() -> Map {
        Map::default()
    }

    pub fn with_capacity(capacity: usize) -> Map {
        Map {
            entries: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Replaces the value in place if `key` exists (returning the old one), else appends.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        let key = key.into();
        let value = value.into();
        match self.get_mut(&key) {
            Some(slot) => Some(std::mem::replace(slot, value)),
            None => {
                self.entries.push((key, value));
                None
            }
        }
    }

    /// Removes `key`, keeping the order of the rest.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(i).1)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, (String, Value)> {
        self.entries.iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    /// Appends a key the caller knows is not present.
    pub(crate) fn push_unique(&mut self, key: String, value: Value) {
        self.entries.push((key, value));
    }

    pub(crate) fn from_parsed(entries: Vec<(String, Value)>) -> Map {
        if !has_duplicates(&entries) {
            return Map { entries };
        }
        let mut out: Vec<(String, Value)> = Vec::with_capacity(entries.len());
        let mut index: HashMap<String, usize> = HashMap::with_capacity(entries.len());
        for (k, v) in entries {
            match index.get(&k) {
                Some(&i) => out[i].1 = v,
                None => {
                    index.insert(k.clone(), out.len());
                    out.push((k, v));
                }
            }
        }
        Map { entries: out }
    }
}

fn has_duplicates(entries: &[(String, Value)]) -> bool {
    if entries.len() <= 16 {
        return (1..entries.len()).any(|i| entries[..i].iter().any(|(k, _)| *k == entries[i].0));
    }
    let mut seen = HashSet::with_capacity(entries.len());
    !entries.iter().all(|(k, _)| seen.insert(k.as_str()))
}

impl IntoIterator for Map {
    type Item = (String, Value);
    type IntoIter = std::vec::IntoIter<(String, Value)>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a> IntoIterator for &'a Map {
    type Item = &'a (String, Value);
    type IntoIter = std::slice::Iter<'a, (String, Value)>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

/// Later duplicates replace earlier values in place, as `insert` does.
impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for Map {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Map {
        let mut m = Map::new();
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}
