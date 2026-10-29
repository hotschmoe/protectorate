use crate::{fields, text};
use protectorate_auth::capability::{self, Caveat};
use protectorate_identity::Store;

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

/// `<token>\0<name>`: verifying a token never panics, an empty store admits
/// nothing, a store admits only the token it issued, and a removed device's
/// token stops verifying.
pub fn device_tokens(data: &[u8]) {
    const NOW: i64 = 1_800_000_000;
    let [token, name] = fields(data, 2)[..] else {
        unreachable!()
    };
    let presented = text(token);
    let store = Store::memory();
    assert!(store.verify_device(&presented, NOW).is_err());
    let Ok((device, issued)) = store.add_device(&text(name), "owner", NOW) else {
        return;
    };
    assert_eq!(
        store.verify_device(&presented, NOW).is_ok(),
        presented == issued.expose(),
        "a token other than the issued one verified"
    );
    assert_eq!(
        store.verify_device(issued.expose(), NOW).unwrap().id,
        device.id
    );
    store.remove_device(&device.id).expect("remove the device");
    assert!(store.verify_device(issued.expose(), NOW).is_err());
}
