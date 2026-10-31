use std::collections::BTreeMap;

use crate::{Error, Map, Number, Value};

pub trait ToJson {
    fn to_json(&self) -> Value;
}

/// Conversion from a borrowed value. Errors carry the path to the failing part.
pub trait FromJson: Sized {
    fn from_json(v: &Value) -> Result<Self, Error>;
}

impl ToJson for Value {
    fn to_json(&self) -> Value {
        self.clone()
    }
}

impl FromJson for Value {
    fn from_json(v: &Value) -> Result<Self, Error> {
        Ok(v.clone())
    }
}

impl ToJson for Map {
    fn to_json(&self) -> Value {
        Value::Object(self.clone())
    }
}

impl FromJson for Map {
    fn from_json(v: &Value) -> Result<Self, Error> {
        v.as_object()
            .cloned()
            .ok_or_else(|| Error::new("expected object"))
    }
}

impl ToJson for bool {
    fn to_json(&self) -> Value {
        Value::Bool(*self)
    }
}

impl FromJson for bool {
    fn from_json(v: &Value) -> Result<Self, Error> {
        v.as_bool().ok_or_else(|| Error::new("expected bool"))
    }
}

impl ToJson for str {
    fn to_json(&self) -> Value {
        Value::String(self.to_owned())
    }
}

impl ToJson for String {
    fn to_json(&self) -> Value {
        Value::String(self.clone())
    }
}

impl FromJson for String {
    fn from_json(v: &Value) -> Result<Self, Error> {
        v.as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::new("expected string"))
    }
}

macro_rules! ints {
    ($via:ident: $($t:ty)*) => {$(
        impl ToJson for $t {
            fn to_json(&self) -> Value {
                Value::Number(Number::from(*self))
            }
        }
        impl FromJson for $t {
            fn from_json(v: &Value) -> Result<Self, Error> {
                v.$via()
                    .and_then(|n| <$t>::try_from(n).ok())
                    .ok_or_else(|| Error::new(concat!("expected ", stringify!($t))))
            }
        }
    )*};
}

ints!(as_i64: i8 i16 i32 i64 isize);
ints!(as_u64: u8 u16 u32 u64 usize);

/// Non-finite values become `null`.
impl ToJson for f64 {
    fn to_json(&self) -> Value {
        Value::from(*self)
    }
}

impl FromJson for f64 {
    fn from_json(v: &Value) -> Result<Self, Error> {
        v.as_f64().ok_or_else(|| Error::new("expected f64"))
    }
}

impl<T: ToJson> ToJson for [T] {
    fn to_json(&self) -> Value {
        Value::Array(self.iter().map(ToJson::to_json).collect())
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn to_json(&self) -> Value {
        self.as_slice().to_json()
    }
}

impl<T: FromJson> FromJson for Vec<T> {
    fn from_json(v: &Value) -> Result<Self, Error> {
        let items = v.as_array().ok_or_else(|| Error::new("expected array"))?;
        items
            .iter()
            .enumerate()
            .map(|(i, x)| T::from_json(x).map_err(|e| e.in_index(i)))
            .collect()
    }
}

impl<T: ToJson> ToJson for Option<T> {
    fn to_json(&self) -> Value {
        self.as_ref().map_or(Value::Null, ToJson::to_json)
    }
}

impl<T: FromJson> FromJson for Option<T> {
    fn from_json(v: &Value) -> Result<Self, Error> {
        match v {
            Value::Null => Ok(None),
            v => T::from_json(v).map(Some),
        }
    }
}

impl<T: ToJson> ToJson for BTreeMap<String, T> {
    fn to_json(&self) -> Value {
        let mut m = Map::with_capacity(self.len());
        for (k, v) in self {
            m.push_unique(k.clone(), v.to_json());
        }
        Value::Object(m)
    }
}

impl<T: FromJson> FromJson for BTreeMap<String, T> {
    fn from_json(v: &Value) -> Result<Self, Error> {
        let m = v.as_object().ok_or_else(|| Error::new("expected object"))?;
        m.iter()
            .map(|(k, x)| Ok((k.clone(), T::from_json(x).map_err(|e| e.in_field(k))?)))
            .collect()
    }
}

impl<T: ToJson + ?Sized> ToJson for &T {
    fn to_json(&self) -> Value {
        (**self).to_json()
    }
}

impl<T: ToJson + ?Sized> ToJson for Box<T> {
    fn to_json(&self) -> Value {
        (**self).to_json()
    }
}

impl<T: FromJson> FromJson for Box<T> {
    fn from_json(v: &Value) -> Result<Self, Error> {
        T::from_json(v).map(Box::new)
    }
}

/// Support for the macros; not a stable API.
#[doc(hidden)]
pub mod __private {
    use crate::{Error, FromJson, Map, ToJson, Value};

    pub fn key(field: &'static str) -> &'static str {
        field.strip_prefix("r#").unwrap_or(field)
    }

    pub fn object(v: &Value) -> Result<&Map, Error> {
        v.as_object().ok_or_else(|| Error::new("expected object"))
    }

    /// Writes a field unless its JSON is null. Field names within one type are unique.
    pub fn put<T: ToJson + ?Sized>(m: &mut Map, key: &'static str, v: &T) {
        let v = v.to_json();
        if !v.is_null() {
            m.push_unique(key.to_owned(), v);
        }
    }

    /// A required field: missing reads as null, so only `Option` and `Value` may be absent.
    pub fn field<T: FromJson>(m: &Map, key: &str) -> Result<T, Error> {
        match m.get(key) {
            Some(v) => T::from_json(v).map_err(|e| e.in_field(key)),
            None => T::from_json(&Value::Null).map_err(|_| Error::new("missing").in_field(key)),
        }
    }

    /// A field with a default: `None` when missing or null.
    pub fn field_or<T: FromJson>(m: &Map, key: &str) -> Result<Option<T>, Error> {
        match m.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => T::from_json(v).map(Some).map_err(|e| e.in_field(key)),
        }
    }

    pub fn tagged(tag: &'static str) -> Map {
        let mut m = Map::new();
        m.push_unique("type".to_owned(), Value::String(tag.to_owned()));
        m
    }

    pub fn tag(m: &Map) -> Result<&str, Error> {
        match m.get("type") {
            Some(Value::String(s)) => Ok(s),
            Some(_) => Err(Error::new("expected string").in_field("type")),
            None => Err(Error::new("missing").in_field("type")),
        }
    }

    pub fn unknown(found: &str, expected: &[&str]) -> Error {
        let shown = match found.char_indices().nth(64) {
            Some((i, _)) => &found[..i],
            None => found,
        };
        Error::new(format!(
            "unknown value {shown:?}, expected one of: {}",
            expected.join(", ")
        ))
    }
}
