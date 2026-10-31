use std::collections::BTreeMap;

use pt_json::{FromJson, Map, Number, ToJson, Value, obj, parse, parse_bytes};

/// Input and its canonical compact form.
const ACCEPT: &[(&str, &str)] = &[
    ("null", "null"),
    (" \t\r\n true \n", "true"),
    ("false", "false"),
    ("0", "0"),
    ("-0", "-0"),
    ("-0.0e-0", "-0.0e-0"),
    ("1.5E+10", "1.5E+10"),
    (
        "123456789012345678901234567890",
        "123456789012345678901234567890",
    ),
    ("1e400", "1e400"),
    ("[ 1 , [ ] , { } , \"\" ]", "[1,[],{},\"\"]"),
    ("{\"a\":1,\"b\":2,\"a\":3}", "{\"a\":3,\"b\":2}"),
    (
        "{\"k0\":0,\"k1\":1,\"k2\":2,\"k3\":3,\"k4\":4,\"k5\":5,\"k6\":6,\"k7\":7,\"k8\":8,\"k9\":9,\"k10\":10,\"k11\":11,\"k12\":12,\"k13\":13,\"k14\":14,\"k15\":15,\"k16\":16,\"k1\":\"x\"}",
        "{\"k0\":0,\"k1\":\"x\",\"k2\":2,\"k3\":3,\"k4\":4,\"k5\":5,\"k6\":6,\"k7\":7,\"k8\":8,\"k9\":9,\"k10\":10,\"k11\":11,\"k12\":12,\"k13\":13,\"k14\":14,\"k15\":15,\"k16\":16}",
    ),
    (
        r#""\" \\ \/ \b \f \n \r \t \u0001 \u001F \u007f""#,
        "\"\\\" \\\\ / \\b \\f \\n \\r \\t \\u0001 \\u001f \u{7f}\"",
    ),
    (
        r#""\u00e9\u4e2D\ud834\udd1e""#,
        "\"\u{e9}\u{4e2d}\u{1d11e}\"",
    ),
    ("\"\u{e9}\u{1d11e} raw\"", "\"\u{e9}\u{1d11e} raw\""),
    (
        "\"a long string with no escapes at all\"",
        "\"a long string with no escapes at all\"",
    ),
    ("\"\\u0000\"", "\"\\u0000\""),
    ("\"\u{2028}\"", "\"\u{2028}\""),
];

/// Input, error offset and message.
const REJECT: &[(&str, usize, &str)] = &[
    ("", 0, "unexpected end of input"),
    ("   ", 3, "unexpected end of input"),
    ("01", 0, "leading zero"),
    ("-01", 1, "leading zero"),
    ("1.", 2, "invalid number"),
    (".5", 0, "expected value"),
    ("-", 1, "invalid number"),
    ("+1", 0, "expected value"),
    ("1e", 2, "invalid number"),
    ("1e+", 3, "invalid number"),
    ("NaN", 0, "expected value"),
    ("Infinity", 0, "expected value"),
    ("tru", 0, "expected value"),
    ("nulll", 4, "trailing characters"),
    ("1 2", 2, "trailing characters"),
    ("{} x", 3, "trailing characters"),
    ("[1,]", 3, "expected value"),
    ("[1 2]", 3, "expected ',' or ']'"),
    ("[", 1, "unexpected end of input"),
    ("{\"a\":1,}", 7, "expected string key"),
    ("{\"a\" 1}", 5, "expected ':'"),
    ("{1:2}", 1, "expected string key"),
    ("{'a':1}", 1, "expected string key"),
    ("{\"a\":1", 6, "unexpected end of input"),
    ("\"abc", 0, "unterminated string"),
    ("\"a\\", 2, "unterminated string"),
    ("\"\\x\"", 1, "invalid escape"),
    ("\"\\u12\"", 5, "invalid unicode escape"),
    ("\"\\ud800\"", 1, "lone surrogate"),
    ("\"\\udc00\"", 1, "lone surrogate"),
    ("\"\\ud800\\u0041\"", 1, "lone surrogate"),
    ("\"\\ud800\\ud800\"", 1, "lone surrogate"),
    ("\"a\u{1}b\"", 2, "control character in string"),
    ("\"tab\there\"", 4, "control character in string"),
    ("\u{feff}1", 0, "expected value"),
];

#[test]
fn accept_and_canonical_form() {
    for (input, want) in ACCEPT {
        let v = parse(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        assert_eq!(v.to_string(), *want, "input {input:?}");
        let mut out = String::new();
        v.write(&mut out);
        assert_eq!(out, *want);
        assert_eq!(parse(want).unwrap(), v, "reparse of {want:?}");
    }
}

#[test]
fn reject_with_offset() {
    for (input, offset, message) in REJECT {
        let e = parse(input).expect_err(input);
        assert_eq!(
            (e.offset(), e.message()),
            (Some(*offset), *message),
            "input {input:?}"
        );
    }
    assert_eq!(
        parse("{} x").unwrap_err().to_string(),
        "at byte 3: trailing characters"
    );
}

#[test]
fn depth_limit_and_bytes() {
    let nest = |n: usize| "[".repeat(n) + &"]".repeat(n);
    assert!(parse(&nest(128)).is_ok());
    assert_eq!(parse(&nest(129)).unwrap_err().offset(), Some(128));
    let objects = "{\"a\":".repeat(129) + "1" + &"}".repeat(129);
    assert_eq!(
        parse(&objects).unwrap_err().message(),
        "nesting deeper than 128"
    );

    assert_eq!(parse_bytes(b"[\"ok\"]").unwrap()[0].as_str(), Some("ok"));
    let e = parse_bytes(b"[\"\xff\"]").unwrap_err();
    assert_eq!((e.offset(), e.message()), (Some(2), "invalid UTF-8"));
}

#[test]
fn numbers() {
    let n = |s: &str| parse(s).unwrap();
    assert_eq!(n("-9223372036854775808").as_i64(), Some(i64::MIN));
    assert_eq!(n("9223372036854775808").as_i64(), None);
    assert_eq!(n("18446744073709551615").as_u64(), Some(u64::MAX));
    assert_eq!(n("18446744073709551616").as_u64(), None);
    assert_eq!(n("-1").as_u64(), None);
    assert_eq!(n("-0").as_u64(), Some(0));
    assert_eq!(n("1e2").as_i64(), None);
    assert_eq!(n("1e2").as_f64(), Some(100.0));
    assert_eq!(n("1.0").as_u64(), None);
    assert_eq!(n("1e400").as_f64(), None);
    assert_eq!(n("1e-400").as_f64(), Some(0.0));
    assert_eq!(n("0.1").as_f64(), Some(0.1));
    assert_ne!(n("1"), n("1.0"));

    let f = |x: f64| Value::from(x).to_string();
    assert_eq!(f(0.1), "0.1");
    assert_eq!(f(1.0), "1.0");
    assert_eq!(f(-0.0), "-0.0");
    assert_eq!(f(1e300), "1e300");
    assert_eq!(f(5e-324), "5e-324");
    assert_eq!(f(f64::NAN), "null");
    assert_eq!(f(f64::INFINITY), "null");
    assert!(Number::from_f64(f64::NEG_INFINITY).is_none());
    assert_eq!(Value::from(u64::MAX).to_string(), "18446744073709551615");
    assert_eq!(Value::from(i64::MIN).to_string(), "-9223372036854775808");
}

#[test]
fn pointer_and_access() {
    let doc = parse(r#"{"a":[{"b":1}],"x/y":2,"m~n":3,"":4,"0":5}"#).unwrap();
    let cases: &[(&str, Option<&str>)] = &[
        ("", Some(r#"{"a":[{"b":1}],"x/y":2,"m~n":3,"":4,"0":5}"#)),
        ("/a/0/b", Some("1")),
        ("/a/0", Some(r#"{"b":1}"#)),
        ("/x~1y", Some("2")),
        ("/m~0n", Some("3")),
        ("/", Some("4")),
        ("/0", Some("5")),
        ("/a/00", None),
        ("/a/-", None),
        ("/a/1", None),
        ("/a/0/b/c", None),
        ("/m~2n", None),
        ("a", None),
    ];
    for (p, want) in cases {
        assert_eq!(
            doc.pointer(p).map(Value::to_string).as_deref(),
            *want,
            "pointer {p:?}"
        );
    }
    assert_eq!(doc["a"][0]["b"].as_u64(), Some(1));
    assert!(doc["nope"][3]["x"].is_null());
    assert_eq!(doc.get("a").and_then(|a| a.at(0)), doc.pointer("/a/0"));
    assert!(Value::from(1).get("a").is_none());

    let mut doc = doc;
    *doc.pointer_mut("/a/0/b").unwrap() = Value::from("z");
    assert_eq!(doc["a"][0]["b"].as_str(), Some("z"));
    let taken = doc.get_mut("a").unwrap().take();
    assert_eq!(taken.to_string(), r#"[{"b":"z"}]"#);
    assert!(doc["a"].is_null());
}

#[test]
fn map_order_and_builders() {
    let mut m = Map::new();
    assert_eq!(m.insert("b", 1), None);
    m.insert("a", 2);
    assert_eq!(m.insert("b", 3).and_then(|v| v.as_u64()), Some(1));
    assert_eq!(m.keys().collect::<Vec<_>>(), ["b", "a"]);
    assert_eq!(m.remove("b").and_then(|v| v.as_u64()), Some(3));
    assert_eq!((m.len(), m.contains_key("b")), (1, false));

    let v = obj! {
        "s" => "x",
        "n" => 1,
        "f" => 2.5,
        "none" => Option::<u32>::None,
        "some" => Some("y"),
        "arr" => vec![Value::from(true), Value::Null],
        "inner" => obj! { "k" => String::from("v") },
        "s" => "replaced",
    };
    assert_eq!(
        v.to_string(),
        r#"{"s":"replaced","n":1,"f":2.5,"none":null,"some":"y","arr":[true,null],"inner":{"k":"v"}}"#
    );
    let collected: Map = [("z", 1), ("y", 2), ("z", 3)].into_iter().collect();
    assert_eq!(Value::from(collected).to_string(), r#"{"z":3,"y":2}"#);
}

pt_json::string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum State {
        Running = "running",
        Stopped = "stopped",
    }
}

pt_json::record! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Sleeve {
        /// Doc comments pass through.
        pub name: String,
        pub state: State,
        pub note: Option<String>,
        pub tags: Vec<String> = Vec::new(),
        pub r#type: u32 = 7,
        pub extra: Value,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Fleet {
        pub sleeves: Vec<Sleeve>,
        pub by_node: BTreeMap<String, u64>,
    }

    #[derive(Debug, Default, PartialEq)]
    struct Empty {}
}

pt_json::tagged_enum! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Event {
        Started = "started" { id: String, at: u64 },
        Stopped = "stopped" { id: String, code: Option<i64>, retries: u32 = 0 },
        Ping = "ping" {},
    }
}

fn from<T: FromJson>(json: &str) -> Result<T, String> {
    pt_json::from_str(json).map_err(|e| e.to_string())
}

#[test]
fn record_macro() {
    let s: Sleeve = from(r#"{"name":"a","state":"running","unknown":[1],"type":3}"#).unwrap();
    assert_eq!(
        s,
        Sleeve {
            name: "a".into(),
            state: State::Running,
            note: None,
            tags: vec![],
            r#type: 3,
            extra: Value::Null,
        }
    );
    assert_eq!(
        s.to_json().to_string(),
        r#"{"name":"a","state":"running","tags":[],"type":3}"#
    );
    let full: Sleeve =
        from(r#"{"name":"b","state":"stopped","note":"n","tags":null,"extra":{"x":1}}"#).unwrap();
    assert_eq!((full.note.as_deref(), full.r#type), (Some("n"), 7));
    assert_eq!(Sleeve::from_json(&full.to_json()).unwrap(), full);

    let errors: &[(&str, &str)] = &[
        (r#"[]"#, "expected object"),
        (r#"{"sleeves":{}}"#, "sleeves: expected array"),
        (r#"{"by_node":{}}"#, "sleeves: missing"),
        (
            r#"{"sleeves":[{"name":"a","state":"running"},{"name":"b","state":"stopped"},{"name":"c","state":1}],"by_node":{}}"#,
            "sleeves[2].state: expected string",
        ),
        (
            r#"{"sleeves":[{"name":"a","state":"paused"}],"by_node":{}}"#,
            r#"sleeves[0].state: unknown value "paused", expected one of: running, stopped"#,
        ),
        (
            r#"{"sleeves":[{"name":"a","state":"running","tags":["x",2]}],"by_node":{}}"#,
            "sleeves[0].tags[1]: expected string",
        ),
        (
            r#"{"sleeves":[],"by_node":{"n1":-1}}"#,
            "by_node.n1: expected u64",
        ),
        (
            r#"{"sleeves":[{"name":null,"state":"running"}],"by_node":{}}"#,
            "sleeves[0].name: expected string",
        ),
    ];
    for (json, want) in errors {
        assert_eq!(from::<Fleet>(json).unwrap_err(), *want, "input {json}");
    }
    let fleet: Fleet = from(r#"{"sleeves":[],"by_node":{"b":2,"a":1}}"#).unwrap();
    assert_eq!(
        fleet.to_json().to_string(),
        r#"{"sleeves":[],"by_node":{"a":1,"b":2}}"#
    );
    assert_eq!(from::<Empty>("{\"x\":1}").unwrap(), Empty {});
    assert_eq!(Empty {}.to_json().to_string(), "{}");
}

#[test]
fn enum_macros() {
    assert_eq!(State::ALL, [State::Running, State::Stopped]);
    assert_eq!("stopped".parse::<State>().unwrap(), State::Stopped);
    assert_eq!(format!("{}", State::Running), "running");
    assert_eq!(State::Stopped.to_json().to_string(), "\"stopped\"");

    let cases: &[(&str, Event)] = &[
        (
            r#"{"type":"started","id":"s1","at":5}"#,
            Event::Started {
                id: "s1".into(),
                at: 5,
            },
        ),
        (
            r#"{"type":"stopped","id":"s2","retries":2}"#,
            Event::Stopped {
                id: "s2".into(),
                code: None,
                retries: 2,
            },
        ),
        (
            r#"{"type":"stopped","id":"s3","code":-1,"retries":0}"#,
            Event::Stopped {
                id: "s3".into(),
                code: Some(-1),
                retries: 0,
            },
        ),
        (r#"{"type":"ping"}"#, Event::Ping {}),
    ];
    for (json, want) in cases {
        let got: Event = from(json).unwrap();
        assert_eq!(&got, want);
        assert_eq!(got.to_json().to_string(), *json);
    }
    let errors: &[(&str, &str)] = &[
        (r#"{"id":"x"}"#, "type: missing"),
        (r#"{"type":1}"#, "type: expected string"),
        (
            r#"{"type":"gone"}"#,
            r#"type: unknown value "gone", expected one of: started, stopped, ping"#,
        ),
        (r#"{"type":"started","id":"x"}"#, "at: missing"),
        (
            r#"{"type":"stopped","id":"x","code":1.5}"#,
            "code: expected i64",
        ),
    ];
    for (json, want) in errors {
        assert_eq!(from::<Event>(json).unwrap_err(), *want, "input {json}");
    }
}

#[test]
fn primitive_conversions() {
    assert_eq!(from::<u8>("256").unwrap_err(), "expected u8");
    assert_eq!(from::<i8>("-128").unwrap(), -128);
    assert_eq!(from::<f64>("1e2").unwrap(), 100.0);
    assert_eq!(from::<Option<bool>>("null").unwrap(), None);
    assert_eq!(
        from::<Vec<Option<u32>>>("[1,null]").unwrap(),
        [Some(1), None]
    );
    assert_eq!(
        from::<Box<String>>("\"b\"").unwrap(),
        Box::new("b".to_owned())
    );
    assert_eq!(vec![Some(1u32), None].to_json().to_string(), "[1,null]");
    assert_eq!("q\"".to_json().to_string(), r#""q\"""#);
    assert_eq!(f64::NAN.to_json(), Value::Null);
}
