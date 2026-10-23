//! WireGuard v1 handshake and transport packet primitives.
//!
//! The caller owns peer lookup, packet delivery, MAC2/cookie defense, source
//! policy, rehandshaking, and network I/O. `Responder` tracks timestamp replay
//! for one configured peer; retain it for the peer's lifetime.

use blake2::digest::consts::U16;
use blake2::digest::{Digest, Mac};
use blake2::{Blake2s256, Blake2sMac};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use curve25519_dalek::montgomery::MontgomeryPoint;
use snow::{params::NoiseParams, Builder, HandshakeState};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

const PROTOCOL: &str = "Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s";
const PROLOGUE: &[u8] = b"WireGuard v1 zx2c4 Jason@zx2c4.com";
const MAX_PACKET: usize = 65_535;
const REJECT_AFTER_MESSAGES: u64 = u64::MAX - (1 << 13);
const REJECT_AFTER_TIME: Duration = Duration::from_secs(180);
const REKEY_AFTER_MESSAGES: u64 = 1 << 60;
const REKEY_AFTER_TIME: Duration = Duration::from_secs(120);
const REPLAY_BITS: u64 = 2048;

pub const INITIATION_SIZE: usize = 148;
pub const RESPONSE_SIZE: usize = 92;
pub const TRANSPORT_HEADER_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidKey,
    InvalidPacket,
    Authentication,
    Replay,
    Expired,
    Random,
    Clock,
    Handshake,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

pub struct Identity {
    private: [u8; 32],
    public: [u8; 32],
}

impl Identity {
    pub fn from_private(private: [u8; 32]) -> Result<Self, Error> {
        if private == [0; 32] {
            return Err(Error::InvalidKey);
        }
        let public = MontgomeryPoint::mul_base_clamped(private).to_bytes();
        Ok(Self { private, public })
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }
}

impl Drop for Identity {
    fn drop(&mut self) {
        self.private.zeroize();
    }
}

fn state(
    local: &Identity,
    peer: Option<&[u8; 32]>,
    psk: &[u8; 32],
    initiator: bool,
) -> Result<HandshakeState, Error> {
    let params: NoiseParams = PROTOCOL.parse().map_err(|_| Error::Handshake)?;
    let mut builder = Builder::new(params)
        .local_private_key(&local.private)
        .prologue(PROLOGUE)
        .psk(2, psk);
    if let Some(peer) = peer {
        builder = builder.remote_public_key(peer);
    }
    if initiator {
        builder.build_initiator().map_err(|_| Error::Handshake)
    } else {
        builder.build_responder().map_err(|_| Error::Handshake)
    }
}

fn mac1_key(receiver: &[u8; 32]) -> [u8; 32] {
    let mut h = Blake2s256::new();
    Digest::update(&mut h, b"mac1----");
    Digest::update(&mut h, receiver);
    h.finalize().into()
}

fn mac1(receiver: &[u8; 32], packet: &[u8]) -> [u8; 16] {
    let mut key = mac1_key(receiver);
    let mut h = <Blake2sMac<U16> as Mac>::new_from_slice(&key).expect("valid BLAKE2s key");
    Mac::update(&mut h, packet);
    let out = h.finalize().into_bytes().into();
    key.zeroize();
    out
}

fn mac1_valid(receiver: &[u8; 32], packet: &[u8], signed_size: usize) -> bool {
    let expected = mac1(receiver, &packet[..signed_size]);
    // Fixed-time comparison avoids exposing the MAC prefix.
    let mut diff = 0u8;
    for (a, b) in expected.iter().zip(&packet[signed_size..signed_size + 16]) {
        diff |= a ^ b;
    }
    diff == 0
}

fn random_index() -> Result<u32, Error> {
    let mut bytes = [0; 4];
    getrandom::getrandom(&mut bytes).map_err(|_| Error::Random)?;
    Ok(u32::from_le_bytes(bytes))
}

pub fn tai64n_now() -> Result<[u8; 12], Error> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Clock)?;
    let secs = elapsed
        .as_secs()
        .checked_add(0x4000_0000_0000_000a)
        .ok_or(Error::Clock)?;
    let mut stamp = [0; 12];
    stamp[..8].copy_from_slice(&secs.to_be_bytes());
    stamp[8..].copy_from_slice(&elapsed.subsec_nanos().to_be_bytes());
    Ok(stamp)
}

pub struct Initiator {
    state: HandshakeState,
    local_index: u32,
    local_public: [u8; 32],
    peer: [u8; 32],
}

impl Initiator {
    pub fn new(
        local: &Identity,
        peer: [u8; 32],
        psk: [u8; 32],
        timestamp: [u8; 12],
    ) -> Result<(Self, [u8; INITIATION_SIZE]), Error> {
        let index = random_index()?;
        Self::with_index(local, peer, psk, timestamp, index)
    }

    /// Explicit index is useful to a packet router that allocates unique indices.
    pub fn with_index(
        local: &Identity,
        peer: [u8; 32],
        psk: [u8; 32],
        timestamp: [u8; 12],
        index: u32,
    ) -> Result<(Self, [u8; INITIATION_SIZE]), Error> {
        if peer == [0; 32] {
            return Err(Error::InvalidKey);
        }
        let mut state = state(local, Some(&peer), &psk, true)?;
        let mut packet = [0u8; INITIATION_SIZE];
        packet[..4].copy_from_slice(&1u32.to_le_bytes());
        packet[4..8].copy_from_slice(&index.to_le_bytes());
        let size = state
            .write_message(&timestamp, &mut packet[8..116])
            .map_err(|_| Error::Handshake)?;
        if size != 108 {
            return Err(Error::Handshake);
        }
        let tag = mac1(&peer, &packet[..116]);
        packet[116..132].copy_from_slice(&tag);
        Ok((
            Self {
                state,
                local_index: index,
                local_public: local.public,
                peer,
            },
            packet,
        ))
    }

    pub fn consume_response(mut self, packet: &[u8]) -> Result<Session, Error> {
        if packet.len() != RESPONSE_SIZE
            || packet[..4] != 2u32.to_le_bytes()
            || packet[8..12] != self.local_index.to_le_bytes()
        {
            return Err(Error::InvalidPacket);
        }
        if !mac1_valid(&self.local_public, packet, 60) {
            return Err(Error::Authentication);
        }
        let mut empty = [];
        let size = self
            .state
            .read_message(&packet[12..60], &mut empty)
            .map_err(|_| Error::Authentication)?;
        if size != 0 {
            return Err(Error::InvalidPacket);
        }
        let remote_index =
            u32::from_le_bytes(packet[4..8].try_into().map_err(|_| Error::InvalidPacket)?);
        Session::from_handshake(&mut self.state, self.local_index, remote_index, true)
    }

    pub fn peer_public_key(&self) -> [u8; 32] {
        self.peer
    }
}

pub struct Responder {
    local: Identity,
    expected_peer: [u8; 32],
    psk: [u8; 32],
    last_timestamp: Option<[u8; 12]>,
}

impl Responder {
    pub fn new(local: Identity, expected_peer: [u8; 32], psk: [u8; 32]) -> Result<Self, Error> {
        if expected_peer == [0; 32] {
            return Err(Error::InvalidKey);
        }
        Ok(Self {
            local,
            expected_peer,
            psk,
            last_timestamp: None,
        })
    }

    pub fn accept(&mut self, packet: &[u8]) -> Result<PendingResponse, Error> {
        if packet.len() != INITIATION_SIZE || packet[..4] != 1u32.to_le_bytes() {
            return Err(Error::InvalidPacket);
        }
        if !mac1_valid(&self.local.public, packet, 116) {
            return Err(Error::Authentication);
        }
        let mut state = state(&self.local, None, &self.psk, false)?;
        let mut timestamp = [0; 12];
        let size = state
            .read_message(&packet[8..116], &mut timestamp)
            .map_err(|_| Error::Authentication)?;
        if size != 12 || state.get_remote_static() != Some(self.expected_peer.as_slice()) {
            return Err(Error::Authentication);
        }
        if self.last_timestamp.is_some_and(|last| timestamp <= last) {
            return Err(Error::Replay);
        }
        self.last_timestamp = Some(timestamp);
        let remote_index =
            u32::from_le_bytes(packet[4..8].try_into().map_err(|_| Error::InvalidPacket)?);
        Ok(PendingResponse {
            state,
            local_public: self.local.public,
            remote_index,
        })
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        self.psk.zeroize();
    }
}

pub struct PendingResponse {
    state: HandshakeState,
    local_public: [u8; 32],
    remote_index: u32,
}

impl PendingResponse {
    pub fn respond(self) -> Result<([u8; RESPONSE_SIZE], Session), Error> {
        self.respond_with_index(random_index()?)
    }

    pub fn respond_with_index(
        mut self,
        index: u32,
    ) -> Result<([u8; RESPONSE_SIZE], Session), Error> {
        let mut packet = [0u8; RESPONSE_SIZE];
        packet[..4].copy_from_slice(&2u32.to_le_bytes());
        packet[4..8].copy_from_slice(&index.to_le_bytes());
        packet[8..12].copy_from_slice(&self.remote_index.to_le_bytes());
        let size = self
            .state
            .write_message(&[], &mut packet[12..60])
            .map_err(|_| Error::Handshake)?;
        if size != 48 {
            return Err(Error::Handshake);
        }
        // Response MAC1 is keyed to the initiator's public key.
        let peer: [u8; 32] = self
            .state
            .get_remote_static()
            .ok_or(Error::Handshake)?
            .try_into()
            .map_err(|_| Error::Handshake)?;
        let tag = mac1(&peer, &packet[..60]);
        packet[60..76].copy_from_slice(&tag);
        let session = Session::from_handshake(&mut self.state, index, self.remote_index, false)?;
        Ok((packet, session))
    }

    pub fn local_public_key(&self) -> [u8; 32] {
        self.local_public
    }
}

pub struct Session {
    send_key: [u8; 32],
    receive_key: [u8; 32],
    local_index: u32,
    remote_index: u32,
    send_counter: u64,
    replay: ReplayWindow,
    created: Instant,
}

impl Session {
    fn from_handshake(
        state: &mut HandshakeState,
        local_index: u32,
        remote_index: u32,
        initiator: bool,
    ) -> Result<Self, Error> {
        if !state.is_handshake_finished() {
            return Err(Error::Handshake);
        }
        let (first, second) = state.dangerously_get_raw_split();
        let (send_key, receive_key) = if initiator {
            (first, second)
        } else {
            (second, first)
        };
        Ok(Self {
            send_key,
            receive_key,
            local_index,
            remote_index,
            send_counter: 0,
            replay: ReplayWindow::default(),
            created: Instant::now(),
        })
    }

    pub fn local_index(&self) -> u32 {
        self.local_index
    }
    pub fn remote_index(&self) -> u32 {
        self.remote_index
    }

    /// The caller should start a fresh handshake before this session expires.
    pub fn needs_rekey(&self) -> bool {
        self.created.elapsed() >= REKEY_AFTER_TIME || self.send_counter >= REKEY_AFTER_MESSAGES
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= REJECT_AFTER_TIME || self.send_counter >= REJECT_AFTER_MESSAGES
    }

    pub fn encrypt(&mut self, payload: &[u8]) -> Result<Vec<u8>, Error> {
        if self.is_expired() {
            return Err(Error::Expired);
        }
        if payload.len() > MAX_PACKET - TRANSPORT_HEADER_SIZE - 16 {
            return Err(Error::InvalidPacket);
        }
        let counter = self.send_counter;
        let cipher = ChaCha20Poly1305::new((&self.send_key).into());
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce(counter)),
                Payload {
                    msg: payload,
                    aad: &[],
                },
            )
            .map_err(|_| Error::Authentication)?;
        let mut packet = Vec::with_capacity(TRANSPORT_HEADER_SIZE + encrypted.len());
        packet.extend_from_slice(&4u32.to_le_bytes());
        packet.extend_from_slice(&self.remote_index.to_le_bytes());
        packet.extend_from_slice(&counter.to_le_bytes());
        packet.extend_from_slice(&encrypted);
        self.send_counter += 1;
        Ok(packet)
    }

    pub fn decrypt(&mut self, packet: &[u8]) -> Result<Vec<u8>, Error> {
        if self.created.elapsed() >= REJECT_AFTER_TIME {
            return Err(Error::Expired);
        }
        if packet.len() < TRANSPORT_HEADER_SIZE + 16
            || packet.len() > MAX_PACKET
            || packet[..4] != 4u32.to_le_bytes()
            || packet[4..8] != self.local_index.to_le_bytes()
        {
            return Err(Error::InvalidPacket);
        }
        let counter =
            u64::from_le_bytes(packet[8..16].try_into().map_err(|_| Error::InvalidPacket)?);
        if counter >= REJECT_AFTER_MESSAGES || !self.replay.can_accept(counter) {
            return Err(Error::Replay);
        }
        let cipher = ChaCha20Poly1305::new((&self.receive_key).into());
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&nonce(counter)),
                Payload {
                    msg: &packet[16..],
                    aad: &[],
                },
            )
            .map_err(|_| Error::Authentication)?;
        self.replay.mark(counter);
        Ok(plaintext)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.send_key.zeroize();
        self.receive_key.zeroize();
    }
}

fn nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&counter.to_le_bytes());
    nonce
}

#[derive(Default)]
struct ReplayWindow {
    highest: Option<u64>,
    bits: [u64; 32],
}

impl ReplayWindow {
    fn can_accept(&self, counter: u64) -> bool {
        match self.highest {
            None => true,
            Some(highest) if counter > highest => true,
            Some(highest) if highest - counter >= REPLAY_BITS => false,
            Some(highest) => {
                self.bits[((highest - counter) / 64) as usize] & (1 << ((highest - counter) % 64))
                    == 0
            }
        }
    }

    fn mark(&mut self, counter: u64) {
        match self.highest {
            None => {
                self.highest = Some(counter);
                self.bits[0] = 1;
            }
            Some(highest) if counter > highest => {
                let shift = counter - highest;
                if shift >= REPLAY_BITS {
                    self.bits = [0; 32];
                } else {
                    let whole = (shift / 64) as usize;
                    let part = (shift % 64) as u32;
                    for i in (0..32).rev() {
                        self.bits[i] = if i >= whole {
                            self.bits[i - whole] << part
                        } else {
                            0
                        };
                        if part != 0 && i > whole {
                            self.bits[i] |= self.bits[i - whole - 1] >> (64 - part);
                        }
                    }
                }
                self.highest = Some(counter);
                self.bits[0] |= 1;
            }
            Some(highest) => {
                let delta = highest - counter;
                self.bits[(delta / 64) as usize] |= 1 << (delta % 64);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_transport_and_replay() {
        let alice = Identity::from_private([1; 32]).unwrap();
        let bob = Identity::from_private([2; 32]).unwrap();
        let mut responder = Responder::new(bob, alice.public_key(), [0; 32]).unwrap();
        let stamp = [1; 12];
        let (initiator, init) =
            Initiator::with_index(&alice, responder.local.public_key(), [0; 32], stamp, 11)
                .unwrap();
        assert_eq!(&init[..8], &[1, 0, 0, 0, 11, 0, 0, 0]);
        assert_eq!(&init[132..], &[0; 16]);
        let pending = responder.accept(&init).unwrap();
        assert!(matches!(responder.accept(&init), Err(Error::Replay)));
        let (response, mut bob_session) = pending.respond_with_index(22).unwrap();
        let mut alice_session = initiator.consume_response(&response).unwrap();
        let first = alice_session.encrypt(b"hello").unwrap();
        assert_eq!(bob_session.decrypt(&first).unwrap(), b"hello");
        assert_eq!(bob_session.decrypt(&first), Err(Error::Replay));
        let reply = bob_session.encrypt(b"world").unwrap();
        assert_eq!(alice_session.decrypt(&reply).unwrap(), b"world");
    }

    #[test]
    fn rejects_modified_handshake_and_ciphertext_without_poisoning_window() {
        let alice = Identity::from_private([3; 32]).unwrap();
        let bob = Identity::from_private([4; 32]).unwrap();
        let mut responder = Responder::new(bob, alice.public_key(), [0; 32]).unwrap();
        let (initiator, init) =
            Initiator::with_index(&alice, responder.local.public_key(), [0; 32], [1; 12], 3)
                .unwrap();
        let mut damaged = init;
        damaged[30] ^= 1;
        assert!(matches!(
            responder.accept(&damaged),
            Err(Error::Authentication)
        ));
        let (response, mut receiver) = responder
            .accept(&init)
            .unwrap()
            .respond_with_index(4)
            .unwrap();
        let mut sender = initiator.consume_response(&response).unwrap();
        let packet = sender.encrypt(b"valid").unwrap();
        let mut damaged = packet.clone();
        *damaged.last_mut().unwrap() ^= 1;
        assert_eq!(receiver.decrypt(&damaged), Err(Error::Authentication));
        assert_eq!(receiver.decrypt(&packet).unwrap(), b"valid");
    }

    #[test]
    fn replay_window_accepts_reordering_and_rejects_aged_packets() {
        let mut window = ReplayWindow::default();
        window.mark(0);
        window.mark(1);
        window.mark(2050);
        assert!(window.can_accept(3));
        assert!(!window.can_accept(2));
        window.mark(3);
        assert!(!window.can_accept(3));
        assert!(!window.can_accept(0));
        window.mark(2115);
        assert!(window.can_accept(2114));
        assert!(!window.can_accept(2050));
    }

    #[test]
    fn nonzero_psk_is_required_to_finish_handshake() {
        let alice = Identity::from_private([5; 32]).unwrap();
        let bob = Identity::from_private([6; 32]).unwrap();
        let (initiator, initiation) =
            Initiator::with_index(&alice, bob.public_key(), [7; 32], [1; 12], 5).unwrap();
        let mut wrong_psk = Responder::new(bob, alice.public_key(), [8; 32]).unwrap();
        let (response, _) = wrong_psk
            .accept(&initiation)
            .unwrap()
            .respond_with_index(6)
            .unwrap();
        assert!(matches!(
            initiator.consume_response(&response),
            Err(Error::Authentication)
        ));
    }
}
