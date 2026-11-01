//! Arbitrary JSON into the FromJson of every request, response and stream type: it must never
//! panic, and whatever parses must come back unchanged through ToJson and FromJson.
//!
//! Seeds: `cargo +nightly fuzz run parse corpus/parse seeds/parse`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_api::{Described, ErrorBody, HubStreamMessage, Method, MethodVisitor, visit_methods};
use pt_json::Value;

fn check<T: Described>(v: &Value) {
    let Ok(x) = T::from_json(v) else {
        return;
    };
    let name = T::def().name;
    let j = x.to_json();
    let y = T::from_json(&j).unwrap_or_else(|e| panic!("{name}: its own JSON fails: {e}\n{j}"));
    assert_eq!(x, y, "{name}: round trip differs\n{j}");
    assert!(y.to_json() == j, "{name}: second write differs\n{j}");
}

struct All<'a>(&'a Value);

impl MethodVisitor for All<'_> {
    fn visit<M: Method>(&mut self) {
        check::<M::Request>(self.0);
        check::<M::Response>(self.0);
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(v) = pt_json::parse_bytes(data) else {
        return;
    };
    visit_methods(&mut All(&v));
    check::<HubStreamMessage>(&v);
    check::<ErrorBody>(&v);
});
