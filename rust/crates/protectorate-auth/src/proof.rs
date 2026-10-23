//! Device request proofs. A device holds a P-256 key whose public half the
//! hub stored at approval, and signs each request: `<device>.<unix
//! seconds>.<nonce>.<signature>`, where the signature covers
//! `<METHOD>\n<path>\n<seconds>\n<nonce>`. It travels as `Authorization:
//! Proof <proof>` or, where a browser cannot set headers, as the WebSocket
//! subprotocol `proof.<proof>`.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
    signature::{self, EcdsaKeyPair, KeyPair},
};

pub const SCHEME: &str = "Proof ";
pub const SUBPROTOCOL: &str = "proof.";
/// How far a proof's clock may be from the hub's, either way.
pub const MAX_SKEW: i64 = 60;
pub const PUBLIC_KEY_LEN: usize = 65;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    pub device: String,
    pub time: i64,
    pub nonce: String,
    pub signature: Vec<u8>,
}

impl Proof {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > 512 {
            return None;
        }
        let mut parts = text.split('.');
        let (device, time, nonce, signature) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some()
            || device.is_empty()
            || device.len() > 64
            || !device.bytes().all(|b| b.is_ascii_hexdigit())
            || !(16..=64).contains(&nonce.len())
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || time.is_empty()
            || time.len() > 12
            || !time.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        Some(Self {
            device: device.into(),
            time: time.parse().ok()?,
            nonce: nonce.into(),
            signature: B64.decode(signature).ok().filter(|s| s.len() <= 80)?,
        })
    }

    /// Checks the signature only; the caller owns clock and replay checks.
    pub fn verify(&self, public_key: &[u8], method: &str, path: &str) -> bool {
        // WebCrypto signs as r||s; Android Keystore signs DER.
        let algorithm = if self.signature.len() == 64 {
            &signature::ECDSA_P256_SHA256_FIXED
        } else {
            &signature::ECDSA_P256_SHA256_ASN1
        };
        signature::UnparsedPublicKey::new(algorithm, public_key)
            .verify(
                message(method, path, self.time, &self.nonce).as_bytes(),
                &self.signature,
            )
            .is_ok()
    }
}

fn message(method: &str, path: &str, time: i64, nonce: &str) -> String {
    format!("{method}\n{path}\n{time}\n{nonce}")
}

/// The device ID the hub derives from its public key.
pub fn device_id(public_key: &[u8]) -> String {
    digest::digest(&digest::SHA256, public_key).as_ref()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn valid_public_key(key: &[u8]) -> bool {
    key.len() == PUBLIC_KEY_LEN && key[0] == 4
}

/// A software device key, for the CLI: PKCS#8 kept in a private file.
pub struct DeviceKey {
    pair: EcdsaKeyPair,
    rng: SystemRandom,
}

impl DeviceKey {
    /// A new key and its PKCS#8 encoding to store.
    pub fn generate() -> Result<(Self, Vec<u8>), String> {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
            .map_err(|_| "generate device key".to_owned())?;
        let key = Self::from_pkcs8(pkcs8.as_ref())?;
        Ok((key, pkcs8.as_ref().to_vec()))
    }

    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, String> {
        let rng = SystemRandom::new();
        let pair =
            EcdsaKeyPair::from_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8, &rng)
                .map_err(|_| "invalid device key".to_owned())?;
        Ok(Self { pair, rng })
    }

    pub fn public_key(&self) -> &[u8] {
        self.pair.public_key().as_ref()
    }

    pub fn id(&self) -> String {
        device_id(self.public_key())
    }

    /// A fresh proof for one request.
    pub fn prove(&self, method: &str, path: &str, time: i64) -> Result<String, String> {
        let mut nonce = [0u8; 16];
        self.rng
            .fill(&mut nonce)
            .map_err(|_| "device proof nonce".to_owned())?;
        let nonce = B64.encode(nonce);
        let signature = self
            .pair
            .sign(&self.rng, message(method, path, time, &nonce).as_bytes())
            .map_err(|_| "sign device proof".to_owned())?;
        Ok(format!(
            "{}.{time}.{nonce}.{}",
            self.id(),
            B64.encode(signature.as_ref())
        ))
    }
}
