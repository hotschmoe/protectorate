//! Arbitrary bytes as a request payload or a member header: never a panic, and whatever parses
//! survives a write and a second parse unchanged.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_frame::msg::{self, PeerHeader, Request};
use pt_json::{FromJson, ToJson};

fuzz_target!(|data: &[u8]| {
    let Ok(v) = pt_json::parse_bytes(data) else {
        return;
    };
    if let Ok(req) = Request::from_json(&v) {
        let again: Request =
            pt_json::from_bytes(&msg::payload(&req)).expect("written request parses");
        assert_eq!(again, req);
    }
    if let Ok(h) = PeerHeader::from_json(&v) {
        let again = PeerHeader::from_json(&h.to_json()).expect("written header parses");
        assert_eq!(again, h);
    }
});
