use protectorate_hub::Hub;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Mints a one-hour capability straight into the hub's store, the way the
/// hub mints for its sleeves, and returns its lineage ID and token.
pub fn mint(hub: &Hub, principal: &str, scopes: &[&str]) -> (String, String) {
    mint_in(&hub.identity, principal, scopes)
}

/// The same, for a hub already moved into its router.
#[allow(dead_code)]
pub fn mint_in(
    store: &protectorate_identity::Store,
    principal: &str,
    scopes: &[&str],
) -> (String, String) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (record, secret) = store
        .mint(
            protectorate_identity::Mint {
                principal,
                scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
                task: "",
                minted_by: "owner",
                ttl: Duration::from_secs(3600),
                file: None,
            },
            now,
        )
        .unwrap();
    (record.id, secret.expose().to_owned())
}
