//! Values built from the schema tables (the ones pt-apigen writes the Dart client from), for a
//! method's request and response or a stream message: each must parse with the Rust type,
//! write JSON that matches the schema, and come back unchanged through ToJson and FromJson.
//! A failure means the schema description and the Rust JSON shape disagree.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_api::{
    Described, FieldDef, HubStreamMessage, METHODS, Method, MethodVisitor, Shape, Ty, TypeDef,
    visit_methods,
};
use pt_json::{Map, Value};

struct Src<'a>(&'a [u8]);

impl Src<'_> {
    fn byte(&mut self) -> u8 {
        match self.0.split_first() {
            Some((b, rest)) => {
                self.0 = rest;
                *b
            }
            None => 0,
        }
    }

    fn u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.byte();
        }
        u64::from_le_bytes(b)
    }

    fn pick<T>(&mut self, xs: &[T]) -> usize {
        usize::from(self.byte()) % xs.len()
    }
}

const CHARS: &[char] = &[
    'a',
    'Z',
    '0',
    ' ',
    '"',
    '\\',
    '/',
    '\n',
    '\t',
    '\u{0}',
    '\u{1f}',
    '\u{7f}',
    '\u{e9}',
    '\u{4e2d}',
    '\u{1d11e}',
    '\u{fffd}',
];

fn build(ty: Ty, s: &mut Src, depth: u32) -> Value {
    match ty {
        Ty::Bool => Value::Bool(s.byte() & 1 == 1),
        Ty::I64 => Value::from(s.u64() as i64),
        Ty::U64 | Ty::Time => Value::from(s.u64()),
        Ty::F64 => {
            let f = f64::from_bits(s.u64());
            Value::from(if f.is_finite() {
                f
            } else {
                f64::from(s.byte()) / 4.0
            })
        }
        Ty::Str => {
            let n = s.byte() % 12;
            Value::String((0..n).map(|_| CHARS[s.pick(CHARS)]).collect())
        }
        Ty::Json => json(s, depth),
        Ty::Opt(t) => match s.byte() % 3 {
            0 => Value::Null,
            _ => build(t(), s, depth),
        },
        Ty::List(t) => {
            let n = if depth > 3 { 0 } else { s.byte() % 4 };
            Value::Array((0..n).map(|_| build(t(), s, depth + 1)).collect())
        }
        Ty::Map(t) => {
            let n = if depth > 3 { 0 } else { s.byte() % 3 };
            let mut m = Map::new();
            for _ in 0..n {
                let k = build(Ty::Str, s, depth);
                m.insert(k.as_str().unwrap_or_default(), build(t(), s, depth + 1));
            }
            Value::Object(m)
        }
        Ty::Named(d) => named(d(), s, depth),
    }
}

fn json(s: &mut Src, depth: u32) -> Value {
    match (s.byte() % 6, depth > 3) {
        (0, _) | (4 | 5, true) => Value::Null,
        (1, _) => build(Ty::Bool, s, depth),
        (2, _) => build(Ty::F64, s, depth),
        (3, _) => build(Ty::Str, s, depth),
        (4, false) => build(Ty::List(|| Ty::Json), s, depth),
        _ => build(Ty::Map(|| Ty::Json), s, depth),
    }
}

fn fields(m: &mut Map, defs: &[FieldDef], s: &mut Src, depth: u32) {
    for f in defs {
        let ty = (f.ty)();
        let omit = match ty {
            Ty::Opt(_) | Ty::Json => s.byte() % 3 == 0,
            _ => f.default.is_some() && s.byte() & 1 == 0,
        };
        if !omit {
            m.insert(f.name, build(ty, s, depth + 1));
        }
    }
}

fn named(def: &TypeDef, s: &mut Src, depth: u32) -> Value {
    match def.shape {
        Shape::Enum(values) => Value::from(values[s.pick(values)].wire),
        Shape::Record(defs) => {
            let mut m = Map::new();
            fields(&mut m, defs, s, depth);
            Value::Object(m)
        }
        Shape::Union(variants) => {
            let v = &variants[s.pick(variants)];
            let mut m = Map::new();
            m.insert("type", v.tag);
            fields(&mut m, v.fields, s, depth);
            Value::Object(m)
        }
    }
}

/// Whether `v` is what the schema says the Rust side writes for `ty`.
fn conforms(ty: Ty, v: &Value, at: &str) -> Result<(), String> {
    let bad = |want: &str| Err(format!("{at}: expected {want}, got {v}"));
    match ty {
        Ty::Bool if v.as_bool().is_some() => Ok(()),
        Ty::I64 if v.as_i64().is_some() => Ok(()),
        Ty::U64 | Ty::Time if v.as_u64().is_some() => Ok(()),
        Ty::F64 if v.as_f64().is_some() => Ok(()),
        Ty::Str if v.as_str().is_some() => Ok(()),
        Ty::Json => Ok(()),
        Ty::Opt(t) if !v.is_null() => conforms(t(), v, at),
        Ty::List(t) => match v.as_array() {
            Some(xs) => xs
                .iter()
                .enumerate()
                .try_for_each(|(i, x)| conforms(t(), x, &format!("{at}[{i}]"))),
            None => bad("array"),
        },
        Ty::Map(t) => match v.as_object() {
            Some(m) => m
                .iter()
                .try_for_each(|(k, x)| conforms(t(), x, &format!("{at}.{k}"))),
            None => bad("object"),
        },
        Ty::Named(d) => {
            let d = d();
            match (d.shape, v) {
                (Shape::Enum(values), Value::String(s)) if values.iter().any(|e| e.wire == s) => {
                    Ok(())
                }
                (Shape::Record(defs), Value::Object(m)) => record(defs, m, at, None),
                (Shape::Union(variants), Value::Object(m)) => {
                    let tag = m.get("type").and_then(Value::as_str);
                    match variants.iter().find(|x| Some(x.tag) == tag) {
                        Some(x) => record(x.fields, m, at, Some("type")),
                        None => bad("a variant tag"),
                    }
                }
                _ => bad(d.name),
            }
        }
        _ => bad("a different type"),
    }
}

fn record(defs: &[FieldDef], m: &Map, at: &str, tag: Option<&str>) -> Result<(), String> {
    for k in m.keys() {
        if Some(k) != tag && !defs.iter().any(|f| f.name == k) {
            return Err(format!("{at}: key {k:?} is not in the schema"));
        }
    }
    for f in defs {
        let ty = (f.ty)();
        match m.get(f.name) {
            Some(Value::Null) => return Err(format!("{at}.{}: written as null", f.name)),
            Some(x) => conforms(ty, x, &format!("{at}.{}", f.name))?,
            None if matches!(ty, Ty::Opt(_) | Ty::Json) => {}
            None => return Err(format!("{at}.{}: required but missing", f.name)),
        }
    }
    Ok(())
}

fn roundtrip<T: Described>(s: &mut Src) {
    let name = T::def().name;
    let built = named(T::def(), s, 0);
    let x = T::from_json(&built)
        .unwrap_or_else(|e| panic!("{name}: a value built from the schema fails: {e}\n{built}"));
    let j = x.to_json();
    if let Err(e) = conforms(Ty::Named(T::def), &j, name) {
        panic!("{name}: written JSON does not match the schema: {e}\n{j}");
    }
    let y = T::from_json(&j).unwrap_or_else(|e| panic!("{name}: its own JSON fails: {e}\n{j}"));
    assert_eq!(x, y, "{name}: round trip differs\n{j}");
    let text = j.to_string();
    let again = pt_json::parse(&text).expect("written JSON parses");
    assert!(again == j, "{name}: text round trip differs\n{text}");
}

struct Pick<'a, 'b> {
    want: usize,
    at: usize,
    src: &'a mut Src<'b>,
}

impl MethodVisitor for Pick<'_, '_> {
    fn visit<M: Method>(&mut self) {
        if self.at == self.want {
            roundtrip::<M::Request>(self.src);
            roundtrip::<M::Response>(self.src);
        }
        self.at += 1;
    }
}

fuzz_target!(|data: &[u8]| {
    let mut src = Src(data);
    let want = usize::from(src.byte()) % (METHODS.len() + 1);
    if want == METHODS.len() {
        roundtrip::<HubStreamMessage>(&mut src);
    } else {
        visit_methods(&mut Pick {
            want,
            at: 0,
            src: &mut src,
        });
    }
});
