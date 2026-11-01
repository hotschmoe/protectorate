//! Release tags against the semver crate: the same tags parse, to the same fields, print back
//! unchanged and order the same. Input is one tag, or two separated by a newline.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pt_update::{Channel, Version};

fn reference(tag: &str) -> Option<semver::Version> {
    let v = semver::Version::parse(tag.strip_prefix('v')?).ok()?;
    v.build.is_empty().then_some(v)
}

fn check(tag: &str) -> Option<(Version, semver::Version)> {
    let ours = Version::parse(tag);
    let theirs = reference(tag);
    assert_eq!(ours.is_some(), theirs.is_some(), "{tag:?}: {ours:?} vs {theirs:?}");
    let (ours, theirs) = (ours?, theirs?);
    assert_eq!(
        (ours.major, ours.minor, ours.patch),
        (theirs.major, theirs.minor, theirs.patch)
    );
    assert_eq!(ours.pre.join("."), theirs.pre.as_str());
    assert_eq!(ours.to_string(), tag);
    assert_eq!(ours.channel() == Channel::Stable, theirs.pre.is_empty());
    assert!(Channel::Dev.admits(&ours));
    Some((ours, theirs))
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (a, b) = text.split_once('\n').unwrap_or((text, ""));
    let (Some(a), Some(b)) = (check(a), check(b)) else {
        return;
    };
    assert_eq!(a.0.cmp(&b.0), a.1.cmp_precedence(&b.1), "{} vs {}", a.0, b.0);
});
