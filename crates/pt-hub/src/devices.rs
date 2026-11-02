//! Paired devices (the apps): `pd1.` bearer tokens, stored only as SHA-256 hashes in
//! `<home>/devices.json`, and the one-time pairing codes `AddDevice` hands out, kept in memory
//! for ten minutes.

use std::io;
use std::sync::Mutex;

use pt_api::{Device, EpochMs};

use crate::home::{Home, now_ms, random_bytes, random_hex, sha_hex, write_private};
use crate::lock::lock;

pub const DEVICE_PREFIX: &str = "pd1.";
const CODE_TTL_MS: u64 = 10 * 60 * 1000;
/// `last_seen_ms` reaches the file at most this often per device.
const SEEN_WRITE_MS: u64 = 60 * 1000;
/// No 0/O or 1/I/L: the code is read off a terminal and typed.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

pt_json::record! {
    #[derive(Debug, Clone, PartialEq)]
    struct Stored {
        id: String,
        name: String,
        created_at_ms: u64,
        last_seen_ms: Option<u64>,
        token_sha256: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct File {
        devices: Vec<Stored> = Vec::new(),
    }
}

struct Code {
    name: String,
    sha: String,
    expires_ms: u64,
}

struct Inner {
    devices: Vec<Stored>,
    /// When each device's `last_seen_ms` last reached the file.
    written: Vec<(String, u64)>,
    codes: Vec<Code>,
}

pub struct Devices {
    home: Home,
    inner: Mutex<Inner>,
}

fn device(s: &Stored) -> Device {
    Device {
        id: s.id.clone(),
        name: s.name.clone(),
        created_at_ms: EpochMs(s.created_at_ms),
        last_seen_ms: s.last_seen_ms.map(EpochMs),
    }
}

/// Upper case, without the dashes and spaces a person may type.
fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

impl Devices {
    pub fn load(home: &Home) -> io::Result<Devices> {
        let path = home.devices();
        let devices = match std::fs::read(&path) {
            Ok(b) => {
                pt_json::from_bytes::<File>(&b)
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("{}: {e}", path.display()),
                        )
                    })?
                    .devices
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                return Err(io::Error::new(
                    e.kind(),
                    format!("read {}: {e}", path.display()),
                ));
            }
        };
        Ok(Devices {
            home: home.clone(),
            inner: Mutex::new(Inner {
                devices,
                written: Vec::new(),
                codes: Vec::new(),
            }),
        })
    }

    fn save(&self, inner: &Inner) -> io::Result<()> {
        let f = File {
            devices: inner.devices.clone(),
        };
        write_private(
            &self.home.devices(),
            pt_json::ToJson::to_json(&f).to_string().as_bytes(),
        )
    }

    pub fn list(&self) -> Vec<Device> {
        lock(&self.inner).devices.iter().map(device).collect()
    }

    /// A new one-time code for a device called `name`, shown as `XXXX-XXXX-XXXX`.
    pub fn add_code(&self, name: &str) -> io::Result<(String, u64)> {
        let raw = random_bytes::<12>()?;
        let code: String = raw
            .iter()
            .map(|b| CODE_ALPHABET[usize::from(*b) % CODE_ALPHABET.len()] as char)
            .collect();
        let expires_ms = now_ms() + CODE_TTL_MS;
        let mut inner = lock(&self.inner);
        let now = now_ms();
        inner.codes.retain(|c| c.expires_ms > now);
        inner.codes.push(Code {
            name: name.to_string(),
            sha: sha_hex(&code),
            expires_ms,
        });
        let shown = format!("{}-{}-{}", &code[..4], &code[4..8], &code[8..]);
        Ok((shown, expires_ms))
    }

    /// Trades a live code for a new device and its token; the code is spent either way.
    pub fn pair(&self, code: &str) -> io::Result<Option<(Device, String)>> {
        let sha = sha_hex(&normalize(code));
        let mut inner = lock(&self.inner);
        let now = now_ms();
        inner.codes.retain(|c| c.expires_ms > now);
        let Some(at) = inner.codes.iter().position(|c| c.sha == sha) else {
            return Ok(None);
        };
        let c = inner.codes.remove(at);
        let token = format!("{DEVICE_PREFIX}{}", random_hex(48)?);
        let s = Stored {
            id: format!("d{}", random_hex(10)?),
            name: c.name,
            created_at_ms: now,
            last_seen_ms: None,
            token_sha256: sha_hex(&token),
        };
        inner.devices.push(s.clone());
        self.save(&inner)?;
        Ok(Some((device(&s), token)))
    }

    /// False when no such device exists.
    pub fn revoke(&self, id: &str) -> io::Result<bool> {
        let mut inner = lock(&self.inner);
        let before = inner.devices.len();
        inner.devices.retain(|d| d.id != id);
        if inner.devices.len() == before {
            return Ok(false);
        }
        self.save(&inner)?;
        Ok(true)
    }

    /// The device holding `token`, noting that it was seen.
    pub fn check(&self, token: &str) -> Option<String> {
        if !token.starts_with(DEVICE_PREFIX) {
            return None;
        }
        let sha = sha_hex(token);
        let mut inner = lock(&self.inner);
        let now = now_ms();
        let d = inner.devices.iter_mut().find(|d| d.token_sha256 == sha)?;
        d.last_seen_ms = Some(now);
        let id = d.id.clone();
        let last = inner
            .written
            .iter()
            .find(|(i, _)| *i == id)
            .map_or(0, |(_, t)| *t);
        if now.saturating_sub(last) >= SEEN_WRITE_MS {
            inner.written.retain(|(i, _)| *i != id);
            inner.written.push((id.clone(), now));
            if let Err(e) = self.save(&inner) {
                crate::log::warning!("devices: {e}");
            }
        }
        Some(id)
    }
}
