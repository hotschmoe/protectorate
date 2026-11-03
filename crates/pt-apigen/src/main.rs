//! Writes the Dart client of the hub API from pt-api's schema tables: immutable classes with
//! `fromJson`/`toJson`, enums that read unknown values as `unknown`, sealed classes for tagged
//! unions, and a `HubApi` with one method per API method plus the events stream.
//!
//! A dev tool, not shipped. `make gen` runs it (`pt-apigen <out.dart>`, stdout without an
//! argument) and formats the result with `dart format`; `make check-gen` fails when the
//! committed file differs.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::Write;
use std::process::exit;

use pt_api::{Described, ErrorBody, FieldDef, HubStreamMessage, METHODS, Shape, Ty, TypeDef};
use pt_json::Value;

const HELPERS: &str = r#"
Never _bad(String at, String want, Object? got) =>
    throw FormatException('$at: expected $want, got ${got.runtimeType}');

Map<String, Object?> _object(Object? v, String at) =>
    v is Map<String, Object?> ? v : _bad(at, 'object', v);

bool _bool(Object? v, String at) => v is bool ? v : _bad(at, 'bool', v);

int _int(Object? v, String at) {
  if (v is int) return v;
  if (v is double && v.isFinite && v == v.roundToDouble()) return v.toInt();
  _bad(at, 'integer', v);
}

double _double(Object? v, String at) => v is num ? v.toDouble() : _bad(at, 'number', v);

String _string(Object? v, String at) => v is String ? v : _bad(at, 'string', v);

DateTime _time(Object? v, String at) => DateTime.fromMillisecondsSinceEpoch(_int(v, at));

List<T> _list<T>(Object? v, String at, T Function(Object?) f) =>
    v is List<Object?> ? List.unmodifiable([for (final e in v) f(e)]) : _bad(at, 'array', v);

Map<String, T> _map<T>(Object? v, String at, T Function(Object?) f) => Map.unmodifiable({
  for (final MapEntry(:key, :value) in _object(v, at).entries) key: f(value),
});

T? _opt<T>(Object? v, T Function(Object?) f) => v == null ? null : f(v);

T _or<T>(Object? v, T fallback, T Function(Object?) f) => v == null ? fallback : f(v);
"#;

const CLIENT: &str = r#"
/// A non-2xx answer from the hub, or a body that is not the API's error shape.
final class ApiException implements Exception {
  const ApiException(this.status, this.code, this.message);

  final int status;
  final ErrorCode code;
  final String message;

  @override
  String toString() => '${code.wire}: $message';
}

ApiException _error(int status, String body) {
  try {
    final e = ErrorBody.fromJson(jsonDecode(body)).error;
    return ApiException(status, e.code, e.message);
  } on FormatException {
    final text = body.length > 200 ? '${body.substring(0, 200)}...' : body;
    return ApiException(status, ErrorCode.unknown, 'HTTP $status $text');
  }
}

/// The hub API: `POST /api/<Method>` per method, the `GET /api/events` WebSocket and the
/// `GET /api/client/<name>` downloads.
/// `baseUrl` is the hub's origin, such as `https://hub.example.lan`; `token` a
/// `pd1.` device token or another bearer credential.
final class HubApi {
  HubApi(this.baseUrl, {this.token});

  final Uri baseUrl;
  final String? token;

  Uri _url(String path, [Map<String, String>? query]) {
    final base = baseUrl.path.endsWith('/')
        ? baseUrl.path.substring(0, baseUrl.path.length - 1)
        : baseUrl.path;
    return baseUrl.replace(path: '$base$path', queryParameters: query);
  }

  Future<T> _call<T>(String method, Map<String, Object?> request, T Function(Object?) parse) async {
    final r = await postJson(_url('/api/$method'), jsonEncode(request), token);
    if (r.status < 200 || r.status > 299) throw _error(r.status, r.body);
    return parse(jsonDecode(r.body));
  }

  /// State changes, starting with a snapshot unless the hub still holds every message from
  /// `fromSeq`. The stream ends when the socket closes; reconnect with the last seq + 1.
  Stream<HubStreamMessage> events({int? fromSeq}) => openSocket(
    _url('/api/events', {if (fromSeq != null) 'from_seq': '$fromSeq'}),
    token,
  ).map((text) => HubStreamMessage.fromJson(jsonDecode(text)));

  /// A client build of the hub's release, one of `HubInfo.clients`: `GET /api/client/<name>`
  /// with this token as the bearer.
  Uri clientUrl(String name) => _url('/api/client/$name');
"#;

const DART_RESERVED: &[&str] = &[
    "assert", "break", "case", "catch", "class", "const", "continue", "default", "do", "else",
    "enum", "extends", "false", "final", "finally", "for", "if", "in", "is", "new", "null",
    "rethrow", "return", "super", "switch", "this", "throw", "true", "try", "var", "void", "while",
    "with", "await", "yield",
];
const ENUM_RESERVED: &[&str] = &["values", "index", "name", "wire", "fromJson", "toJson"];
const MEMBER_RESERVED: &[&str] = &["toJson", "hashCode", "runtimeType", "toString", "json"];

fn fail(msg: String) -> ! {
    eprintln!("pt-apigen: {msg}");
    exit(1)
}

fn main() {
    let code = generate();
    match std::env::args().nth(1) {
        Some(path) => {
            std::fs::write(&path, code).unwrap_or_else(|e| fail(format!("writing {path}: {e}")))
        }
        None => print!("{code}"),
    }
}

fn generate() -> String {
    let mut types = BTreeMap::new();
    collect(HubStreamMessage::def(), &mut types);
    collect(ErrorBody::def(), &mut types);
    for m in METHODS {
        collect((m.request)(), &mut types);
        collect((m.response)(), &mut types);
    }

    let mut out = String::new();
    for def in types.values() {
        out.push('\n');
        match def.shape {
            Shape::Enum(values) => enum_class(&mut out, def, values),
            Shape::Record(fields) => record_class(&mut out, def, fields),
            Shape::Union(_) => union_classes(&mut out, def),
        }
    }
    out.push_str(CLIENT);
    for m in METHODS {
        let req = (m.request)();
        let resp = (m.response)();
        let name = lower_first(m.name);
        out.push('\n');
        doc(&mut out, m.doc);
        if !m.doc.is_empty() {
            out.push_str("  ///\n");
        }
        let _ = writeln!(out, "  /// `POST /api/{}`, scope `{}`.", m.name, m.scope);
        let param = if is_empty_record(req) {
            format!("[{0} request = const {0}()]", req.name)
        } else {
            format!("{} request", req.name)
        };
        let _ = writeln!(
            out,
            "  Future<{resp}> {name}({param}) =>\n      _call('{method}', request.toJson(), {resp}.fromJson);",
            resp = resp.name,
            method = m.name,
        );
    }
    out.push_str("}\n");

    let mut head = String::from(
        "// GENERATED by pt-apigen from crates/pt-api. Do not edit; run `make gen`.\n\n\
         import 'dart:convert';\n\nimport 'transport.dart';\n",
    );
    let helpers: Vec<&str> = HELPERS.trim().split("\n\n").collect();
    let name_of = |h: &str| -> String {
        let sig = h.split('(').next().unwrap_or_default();
        let name = sig.rsplit(' ').next().unwrap_or_default();
        name.split('<').next().unwrap_or_default().to_owned()
    };
    let used = |name: &str, text: &str| text.contains(&format!("{name}("));
    for h in &helpers {
        let name = name_of(h);
        let by_others = helpers
            .iter()
            .any(|o| name_of(o) != name && used(&name_of(o), &out) && used(&name, o));
        if used(&name, &out) || by_others {
            head.push('\n');
            head.push_str(h);
            head.push('\n');
        }
    }
    head.push_str(&out);
    head
}

fn collect(def: &'static TypeDef, seen: &mut BTreeMap<&'static str, &'static TypeDef>) {
    if let Some(prev) = seen.get(def.name) {
        if !std::ptr::eq(*prev, def) {
            fail(format!("two API types are named {}", def.name));
        }
        return;
    }
    seen.insert(def.name, def);
    let fields: Vec<&FieldDef> = match def.shape {
        Shape::Record(f) => f.iter().collect(),
        Shape::Union(v) => v.iter().flat_map(|v| v.fields.iter()).collect(),
        Shape::Enum(_) => Vec::new(),
    };
    for f in fields {
        collect_ty((f.ty)(), seen);
    }
}

fn collect_ty(ty: Ty, seen: &mut BTreeMap<&'static str, &'static TypeDef>) {
    match ty {
        Ty::Opt(t) | Ty::List(t) | Ty::Map(t) => collect_ty(t(), seen),
        Ty::Named(d) => collect(d(), seen),
        _ => {}
    }
}

fn is_empty_record(def: &TypeDef) -> bool {
    matches!(def.shape, Shape::Record(f) if f.is_empty())
}

// ---- names and docs ---------------------------------------------------------------------------

fn camel(snake: &str) -> String {
    let mut out = String::new();
    for (i, part) in snake.split('_').filter(|p| !p.is_empty()).enumerate() {
        if i == 0 {
            out.push_str(part);
        } else {
            let mut c = part.chars();
            if let Some(first) = c.next() {
                out.extend(first.to_uppercase());
                out.push_str(c.as_str());
            }
        }
    }
    out
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

fn check_ident(name: &str, extra: &[&str], what: &str) {
    if DART_RESERVED.contains(&name) || extra.contains(&name) {
        fail(format!(
            "{what}: `{name}` is reserved in the Dart client; rename it"
        ));
    }
}

fn doc(out: &mut String, text: &str) {
    for line in text.lines() {
        let line = line.strip_prefix(' ').unwrap_or(line).trim_end();
        if line.is_empty() {
            out.push_str("///\n");
        } else {
            let _ = writeln!(out, "/// {line}");
        }
    }
}

fn dart_string(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

// ---- types --------------------------------------------------------------------------------------

fn dart_type(ty: Ty) -> String {
    match ty {
        Ty::Bool => "bool".into(),
        Ty::I64 | Ty::U64 => "int".into(),
        Ty::F64 => "double".into(),
        Ty::Str => "String".into(),
        Ty::Time => "DateTime".into(),
        Ty::Json => "Object?".into(),
        Ty::Opt(t) => match t() {
            Ty::Json => "Object?".into(),
            t => format!("{}?", dart_type(t)),
        },
        Ty::List(t) => format!("List<{}>", dart_type(t())),
        Ty::Map(t) => format!("Map<String, {}>", dart_type(t())),
        Ty::Named(d) => d().name.into(),
    }
}

/// An expression of type `T Function(Object?)`.
fn parser(ty: Ty, at: &str, depth: usize) -> String {
    match ty {
        Ty::Named(d) => format!("{}.fromJson", d().name),
        ty => format!(
            "(e{depth}) => {}",
            parse_expr(ty, &format!("e{depth}"), at, depth + 1)
        ),
    }
}

/// Parses `src`, an `Object?` expression.
fn parse_expr(ty: Ty, src: &str, at: &str, depth: usize) -> String {
    let at_lit = dart_string(at);
    match ty {
        Ty::Bool => format!("_bool({src}, {at_lit})"),
        Ty::I64 | Ty::U64 => format!("_int({src}, {at_lit})"),
        Ty::F64 => format!("_double({src}, {at_lit})"),
        Ty::Str => format!("_string({src}, {at_lit})"),
        Ty::Time => format!("_time({src}, {at_lit})"),
        Ty::Json => src.into(),
        Ty::Opt(t) => match t() {
            Ty::Json => src.into(),
            t => format!("_opt({src}, {})", parser(t, at, depth)),
        },
        Ty::List(t) => format!(
            "_list({src}, {at_lit}, {})",
            parser(t(), &format!("{at}[]"), depth)
        ),
        Ty::Map(t) => format!(
            "_map({src}, {at_lit}, {})",
            parser(t(), &format!("{at}{{}}"), depth)
        ),
        Ty::Named(d) => format!("{}.fromJson({src})", d().name),
    }
}

/// Encodes `src`, an expression of the Dart type of `ty`, to JSON.
fn encode(ty: Ty, src: &str, depth: usize) -> String {
    let e = format!("e{depth}");
    match ty {
        Ty::Bool | Ty::I64 | Ty::U64 | Ty::F64 | Ty::Str | Ty::Json => src.into(),
        Ty::Time => format!("{src}.millisecondsSinceEpoch"),
        Ty::Named(_) => format!("{src}.toJson()"),
        Ty::List(t) => {
            let inner = encode(t(), &e, depth + 1);
            if inner == e {
                src.into()
            } else {
                format!("[for (final {e} in {src}) {inner}]")
            }
        }
        Ty::Map(t) => {
            let inner = encode(t(), &e, depth + 1);
            if inner == e {
                src.into()
            } else {
                format!(
                    "{{for (final MapEntry(key: k{depth}, value: {e}) in {src}.entries) k{depth}: {inner}}}"
                )
            }
        }
        Ty::Opt(t) => {
            let inner = encode(t(), &e, depth + 1);
            if inner == e {
                src.into()
            } else {
                format!("switch ({src}) {{ null => null, final {e} => {inner} }}")
            }
        }
    }
}

/// A Dart constant for a field default.
fn dart_const(ty: Ty, v: &Value, at: &str) -> String {
    let bad = || -> ! { fail(format!("{at}: default {v} has no Dart constant form")) };
    match (ty, v) {
        (Ty::Bool, Value::Bool(b)) => b.to_string(),
        (Ty::I64 | Ty::U64 | Ty::F64, Value::Number(n)) => n.as_str().to_owned(),
        (Ty::Str, Value::String(s)) => dart_string(s),
        (Ty::Named(d), Value::String(s)) => match d().shape {
            Shape::Enum(_) => format!("{}.{}", d().name, camel(s)),
            _ => bad(),
        },
        (Ty::List(t), Value::Array(items)) => format!(
            "const <{}>[{}]",
            dart_type(t()),
            items
                .iter()
                .map(|x| dart_const(t(), x, at)
                    .trim_start_matches("const ")
                    .to_owned())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (Ty::Map(t), Value::Object(m)) => format!(
            "const <String, {}>{{{}}}",
            dart_type(t()),
            m.iter()
                .map(|(k, x)| format!(
                    "{}: {}",
                    dart_string(k),
                    dart_const(t(), x, at).trim_start_matches("const ")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => bad(),
    }
}

struct Field {
    key: &'static str,
    name: String,
    ty: Ty,
    dtype: String,
    default: Option<String>,
    doc: &'static str,
    at: String,
}

impl Field {
    fn new(owner: &str, f: &FieldDef) -> Field {
        let ty = (f.ty)();
        let at = format!("{owner}.{}", f.name);
        let time = matches!(ty, Ty::Time) || matches!(ty, Ty::Opt(t) if matches!(t(), Ty::Time));
        let base = if time {
            f.name
                .strip_suffix("_ms")
                .unwrap_or_else(|| fail(format!("{at}: a time field's name must end in _ms")))
        } else {
            f.name
        };
        let name = camel(base);
        check_ident(&name, MEMBER_RESERVED, &at);
        if f.default.is_some() && matches!(ty, Ty::Opt(_) | Ty::Json) {
            fail(format!("{at}: an optional field cannot have a default"));
        }
        Field {
            key: f.name,
            dtype: dart_type(ty),
            default: f.default.map(|d| dart_const(ty, &d(), &at)),
            name,
            ty,
            doc: f.doc,
            at,
        }
    }

    fn optional(&self) -> bool {
        matches!(self.ty, Ty::Opt(_) | Ty::Json)
    }

    fn param(&self) -> String {
        match &self.default {
            Some(d) => format!("this.{} = {d}", self.name),
            None if self.optional() => format!("this.{}", self.name),
            None => format!("required this.{}", self.name),
        }
    }

    /// Reads this field from the map expression `map`.
    fn parse(&self, map: &str) -> String {
        let src = format!("{map}[{}]", dart_string(self.key));
        match (&self.default, self.ty) {
            (Some(d), ty) => format!("_or({src}, {d}, {})", parser(ty, &self.at, 0)),
            (None, ty) => parse_expr(ty, &src, &self.at, 0),
        }
    }

    fn to_json(&self) -> String {
        let key = dart_string(self.key);
        match self.ty {
            Ty::Opt(t) => format!(
                "if ({} case final v?) {key}: {}",
                self.name,
                encode(t(), "v", 0)
            ),
            Ty::Json => format!("if ({} case final v?) {key}: v", self.name),
            ty => format!("{key}: {}", encode(ty, &self.name, 0)),
        }
    }
}

fn fields_of(owner: &str, defs: &[FieldDef]) -> Vec<Field> {
    defs.iter().map(|f| Field::new(owner, f)).collect()
}

// ---- classes ------------------------------------------------------------------------------------

fn enum_class(out: &mut String, def: &TypeDef, values: &[pt_api::EnumValue]) {
    doc(out, def.doc);
    let _ = writeln!(out, "enum {} {{", def.name);
    let mut has_unknown = false;
    for v in values {
        let name = camel(v.wire);
        check_ident(&name, ENUM_RESERVED, &format!("{}.{}", def.name, v.wire));
        has_unknown |= name == "unknown";
        doc(out, v.doc);
        let _ = writeln!(out, "{name}({}),", dart_string(v.wire));
    }
    if !has_unknown {
        out.push_str("/// A value this client does not know.\nunknown('unknown'),\n");
    }
    out.push_str(";\n");
    let _ = writeln!(
        out,
        "const {name}(this.wire);\n\n\
         final String wire;\n\n\
         static {name} fromJson(Object? json) {{\n\
           for (final v in values) {{\n\
             if (v.wire == json) return v;\n\
           }}\n\
           return unknown;\n\
         }}\n\n\
         String toJson() => wire;\n}}",
        name = def.name
    );
}

fn class_body(
    out: &mut String,
    class: &str,
    fields: &[Field],
    lifted: &[String],
    tag: Option<&str>,
) {
    if fields.is_empty() {
        let _ = writeln!(out, "const {class}();");
    } else {
        let params: Vec<String> = fields.iter().map(Field::param).collect();
        let _ = writeln!(out, "const {class}({{{},}});", params.join(", "));
    }
    out.push('\n');
    let (ctor, map) = match tag {
        Some(_) => (
            format!("factory {class}._fromMap(Map<String, Object?> j)"),
            "j",
        ),
        None => (format!("factory {class}.fromJson(Object? json)"), "j"),
    };
    if fields.is_empty() {
        match tag {
            Some(_) => {
                let _ = writeln!(out, "{ctor} => const {class}();");
            }
            None => {
                let _ = writeln!(
                    out,
                    "{ctor} {{\n_object(json, {});\nreturn const {class}();\n}}",
                    dart_string(class)
                );
            }
        }
    } else {
        let args: Vec<String> = fields
            .iter()
            .map(|f| format!("{}: {}", f.name, f.parse(map)))
            .collect();
        match tag {
            Some(_) => {
                let _ = writeln!(out, "{ctor} => {class}({},);", args.join(", "));
            }
            None => {
                let _ = writeln!(
                    out,
                    "{ctor} {{\nfinal j = _object(json, {});\nreturn {class}({},);\n}}",
                    dart_string(class),
                    args.join(", ")
                );
            }
        }
    }
    for f in fields {
        out.push('\n');
        doc(out, f.doc);
        if lifted.contains(&f.name) {
            out.push_str("@override\n");
        }
        let _ = writeln!(out, "final {} {};", f.dtype, f.name);
    }
    out.push('\n');
    if tag.is_some() {
        out.push_str("@override\n");
    }
    let mut entries: Vec<String> = Vec::new();
    if let Some(tag) = tag {
        entries.push(format!("'type': {}", dart_string(tag)));
    }
    entries.extend(fields.iter().map(Field::to_json));
    if entries.is_empty() {
        out.push_str("Map<String, Object?> toJson() => const {};\n");
    } else {
        let _ = writeln!(
            out,
            "Map<String, Object?> toJson() => {{{},}};",
            entries.join(", ")
        );
    }
    out.push_str("}\n");
}

fn record_class(out: &mut String, def: &TypeDef, defs: &[FieldDef]) {
    let fields = fields_of(def.name, defs);
    doc(out, def.doc);
    let _ = writeln!(out, "final class {} {{", def.name);
    class_body(out, def.name, &fields, &[], None);
}

fn union_classes(out: &mut String, def: &TypeDef) {
    let Shape::Union(variants) = def.shape else {
        unreachable!()
    };
    let name = def.name;
    let all: Vec<Vec<Field>> = variants
        .iter()
        .map(|v| fields_of(&format!("{name}{}", v.name), v.fields))
        .collect();
    // Fields every variant has with the same key and type become getters of the base class.
    let lifted: Vec<&Field> = all
        .first()
        .map(|first| {
            first
                .iter()
                .filter(|f| {
                    all.iter().all(|fs| {
                        fs.iter()
                            .any(|g| g.key == f.key && g.dtype == f.dtype && g.default == f.default)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let lifted_names: Vec<String> = lifted.iter().map(|f| f.name.clone()).collect();

    doc(out, def.doc);
    let _ = writeln!(out, "sealed class {name} {{\nconst {name}();\n");
    let _ = writeln!(
        out,
        "factory {name}.fromJson(Object? json) {{\nfinal j = _object(json, {});\nreturn switch (j['type']) {{",
        dart_string(name)
    );
    for v in variants {
        let _ = writeln!(
            out,
            "{} => {name}{}._fromMap(j),",
            dart_string(v.tag),
            v.name
        );
    }
    let _ = writeln!(out, "_ => {name}Unknown(j),\n}};\n}}");
    for f in &lifted {
        out.push('\n');
        doc(out, f.doc);
        let _ = writeln!(out, "{} get {};", f.dtype, f.name);
    }
    out.push_str("\nMap<String, Object?> toJson();\n}\n");

    for (v, fields) in variants.iter().zip(&all) {
        let class = format!("{name}{}", v.name);
        out.push('\n');
        doc(out, v.doc);
        let _ = writeln!(out, "final class {class} extends {name} {{");
        class_body(out, &class, fields, &lifted_names, Some(v.tag));
    }

    let _ = writeln!(
        out,
        "\n/// A variant this client does not know; `json` is the object as received.\n\
         final class {name}Unknown extends {name} {{\nconst {name}Unknown(this.json);\n\n\
         final Map<String, Object?> json;"
    );
    for f in &lifted {
        let _ = writeln!(
            out,
            "\n@override\n{} get {} => {};",
            f.dtype,
            f.name,
            f.parse("json").replace(&f.at, &format!("{name}.{}", f.key))
        );
    }
    out.push_str("\n@override\nMap<String, Object?> toJson() => json;\n}\n");
}
