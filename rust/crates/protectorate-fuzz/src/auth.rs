use crate::{fields, text};
use protectorate_auth::{
    capability::{self, Caveat},
    proof::{DeviceKey, Proof},
};
use std::sync::OnceLock;

const KEY: &[u8] = b"protectorate fuzz capability key";

/// `<token>\0<caveat>`: decoding never panics and agrees with verification;
/// reissuing decoded claims verifies with the same claims; appending a caveat
/// keeps a token verifying and only narrows it.
pub fn capability_tokens(data: &[u8]) {
    let [token, caveat] = fields(data, 2)[..] else {
        unreachable!()
    };
    let token = text(token);
    let decoded = capability::decode(&token);
    if let Ok(claims) = capability::verify(KEY, &token) {
        assert_eq!(Ok(&claims), decoded.as_ref(), "verify and decode disagree");
    }
    let Ok(claims) = decoded else {
        return;
    };
    let Ok(minted) = capability::reissue(KEY, &claims, claims.issued, claims.expires) else {
        return;
    };
    let again = capability::verify(KEY, &minted).expect("a reissued capability verifies");
    assert_eq!(again, claims, "reissue changed the claims");
    assert!(capability::verify(b"another key", &minted).is_err());
    let caveat = text(caveat);
    let caveat = match caveat.strip_prefix("exp=") {
        Some(at) => match at.parse() {
            Ok(at) => Caveat::Expires(at),
            Err(_) => return,
        },
        None => Caveat::Scopes(caveat.split(' ').map(str::to_owned).collect()),
    };
    let Ok(narrowed) = capability::attenuate(&minted, &caveat) else {
        return;
    };
    let narrowed = capability::verify(KEY, &narrowed).expect("an attenuated capability verifies");
    assert_eq!(narrowed.id, claims.id);
    assert_eq!(narrowed.scopes, claims.scopes);
    assert!(
        narrowed.expires <= claims.expires,
        "attenuation extended expiry"
    );
    assert!(narrowed.narrowed.len() >= claims.narrowed.len());
}

/// `<proof>\0<method>\0<path>`: parsing never panics, and a fresh proof
/// signed by a device key verifies for its own method and path only.
pub fn device_proofs(data: &[u8]) {
    static DEVICE: OnceLock<DeviceKey> = OnceLock::new();
    let [proof, method, path] = fields(data, 3)[..] else {
        unreachable!()
    };
    let key = DEVICE.get_or_init(|| DeviceKey::generate().expect("device key").0);
    let (method, path) = (text(method), text(path));
    if let Some(proof) = Proof::parse(&text(proof)) {
        let _ = proof.verify(key.public_key(), &method, &path);
    }
    let signed = key
        .prove(&method, &path, 1_800_000_000)
        .expect("sign a proof");
    let parsed = Proof::parse(&signed).expect("a signed proof parses");
    assert_eq!(parsed.device, key.id());
    assert!(parsed.verify(key.public_key(), &method, &path));
    assert!(!parsed.verify(key.public_key(), &method, &format!("{path}/")));
}
