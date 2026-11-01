//! Arbitrary bytes as every answer payload the hub reads from a node (which may be hostile):
//! the hello, a job status, a list, both end frames, an update answer and an error. Never a
//! panic, and whatever parses survives a write and a second parse unchanged, the submit token
//! included.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_frame::msg::{
    self, ArtifactEnd, ErrorReply, HelloReply, JobList, JobReply, LogEnd, UpdateReply,
};
use pt_json::{FromJson, ToJson};

fn again<T: FromJson + ToJson + PartialEq + std::fmt::Debug>(v: &pt_json::Value) {
    if let Ok(t) = T::from_json(v) {
        let back: T = pt_json::from_bytes(&msg::payload(&t)).expect("a written answer parses");
        assert_eq!(back, t);
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(v) = pt_json::parse_bytes(data) else {
        return;
    };
    again::<HelloReply>(&v);
    again::<JobReply>(&v);
    again::<JobList>(&v);
    again::<LogEnd>(&v);
    again::<ArtifactEnd>(&v);
    again::<UpdateReply>(&v);
    again::<ErrorReply>(&v);
});
