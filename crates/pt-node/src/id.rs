/// 128 bits as 32 hex digits, from std's randomly keyed SipHash (no CSPRNG crate): unique enough
/// for node ids and journal epochs, never for secrets.
pub fn random_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut out = String::with_capacity(32);
    for i in 0..2u8 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u8(i);
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}
