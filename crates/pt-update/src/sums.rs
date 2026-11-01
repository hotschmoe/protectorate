use crate::{Error, ErrorKind, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// A parsed `SHA256SUMS` in sha256sum's format: `<64 lowercase hex>  <name>` per line (or
/// ` *<name>` for binary mode). Names are plain file names; a repeated name is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sums(BTreeMap<String, [u8; 32]>);

fn bad(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Verify, msg)
}

pub(crate) fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 255
        && !name.starts_with(char::is_whitespace)
        && !name.ends_with(char::is_whitespace)
        && !name.contains(['/', '\\'])
        && !name.contains(char::is_control)
}

impl Sums {
    pub fn parse(bytes: &[u8]) -> Result<Sums> {
        let text = std::str::from_utf8(bytes).map_err(|_| bad("SHA256SUMS is not UTF-8"))?;
        let mut map = BTreeMap::new();
        for (i, line) in text.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let at = || format!("SHA256SUMS line {}", i + 1);
            let (hex, rest) = line
                .split_at_checked(64)
                .ok_or_else(|| bad(format!("{}: too short", at())))?;
            let digest =
                pt_hash::from_hex_array::<32>(hex).map_err(|e| bad(format!("{}: {e}", at())))?;
            let name = rest
                .strip_prefix("  ")
                .or_else(|| rest.strip_prefix(" *"))
                .ok_or_else(|| bad(format!("{}: want two spaces after the digest", at())))?;
            if !plain_name(name) {
                return Err(bad(format!("{}: {name:?} is not a plain file name", at())));
            }
            if map.insert(name.to_string(), digest).is_some() {
                return Err(bad(format!("{}: {name} is listed twice", at())));
            }
        }
        if map.is_empty() {
            return Err(bad("SHA256SUMS has no entries"));
        }
        Ok(Sums(map))
    }

    pub fn get(&self, name: &str) -> Option<&[u8; 32]> {
        self.0.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    fn want(&self, name: &str) -> Result<&[u8; 32]> {
        self.get(name)
            .ok_or_else(|| bad(format!("SHA256SUMS has no entry for {name}")))
    }

    fn compare(name: &str, want: &[u8; 32], got: &[u8; 32]) -> Result<()> {
        if want == got {
            return Ok(());
        }
        Err(bad(format!(
            "{name}: sha256 {} does not match SHA256SUMS ({})",
            pt_hash::to_hex(got),
            pt_hash::to_hex(want)
        )))
    }

    pub fn verify_bytes(&self, name: &str, bytes: &[u8]) -> Result<()> {
        Sums::compare(name, self.want(name)?, &pt_hash::sha256(bytes))
    }

    pub fn verify_file(&self, name: &str, path: &Path) -> Result<()> {
        let want = self.want(name)?;
        let got = pt_hash::sha256_file(path)
            .map_err(|e| Error::io(format!("hashing {}", path.display()), e))?;
        Sums::compare(name, want, &got)
    }
}
