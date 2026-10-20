//! SFrame ([RFC 9605](https://www.rfc-editor.org/rfc/rfc9605)) frame encryption with cipher suite
//! `AES_128_GCM_SHA256_128`, keyed from the MLS exporter as in its section 5.2: every epoch's
//! [`EXPORTER_LABEL`](crate::sframe::EXPORTER_LABEL) secret is the base key, and each member
//! encrypts under its own KID, so no two senders share a key. The nonce is the frame counter XOR
//! the salt, never random; a receiver drops a frame whose counter it has already accepted or that
//! is older than [`REPLAY_WINDOW`](crate::sframe::REPLAY_WINDOW).

use crate::VideoError;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use std::collections::HashMap;

/// The SFrame cipher suite, `AES_128_GCM_SHA256_128`.
pub const CIPHER_SUITE: u16 = 0x0004;
/// The MLS exporter label of the base key; the exporter context is empty.
pub const EXPORTER_LABEL: &str = "SFrame 1.0 Base Key";
/// Bytes of the base key to export (`AEAD.Nk`).
pub const BASE_KEY_BYTES: usize = 16;
/// Low-order epoch bits in a KID (`E`): up to this many epochs' keys are told apart.
pub const EPOCH_BITS: u32 = 4;
/// Counters a receiver remembers below the highest one it accepted.
pub const REPLAY_WINDOW: u64 = 64;
const NONCE_BYTES: usize = 12;

/// The KID of member `index` in MLS `epoch`, with context 0.
pub fn kid(epoch: u64, index: u32) -> u64 {
    (u64::from(index) << EPOCH_BITS) | (epoch & ((1 << EPOCH_BITS) - 1))
}

struct Key {
    cipher: Aes128Gcm,
    salt: [u8; NONCE_BYTES],
}

impl Key {
    fn derive(base_key: &[u8], kid: u64) -> Result<Self, VideoError> {
        let (_, secret) = Hkdf::<Sha256>::extract(None, base_key);
        let expand = |label: &str, out: &mut [u8]| {
            let info = [
                label.as_bytes(),
                &kid.to_be_bytes(),
                &CIPHER_SUITE.to_be_bytes(),
            ]
            .concat();
            secret
                .expand(&info, out)
                .map_err(|_| VideoError::EncryptionError("key derivation".into()))
        };
        let mut key = [0; 16];
        let mut salt = [0; NONCE_BYTES];
        expand("SFrame 1.0 Secret key ", &mut key)?;
        expand("SFrame 1.0 Secret salt ", &mut salt)?;
        Ok(Self {
            cipher: Aes128Gcm::new(&key.into()),
            salt,
        })
    }

    fn nonce(&self, ctr: u64) -> Nonce<aes_gcm::aead::consts::U12> {
        let mut nonce = self.salt;
        for (byte, ctr) in nonce[NONCE_BYTES - 8..].iter_mut().zip(ctr.to_be_bytes()) {
            *byte ^= ctr;
        }
        nonce.into()
    }
}

/// This member's outgoing frames in one epoch; every frame takes the next counter.
pub struct Sender {
    key: Key,
    kid: u64,
    ctr: u64,
}

impl Sender {
    /// The sender of member `index` in MLS `epoch`, whose exporter gave `base_key`.
    pub fn new(base_key: &[u8], epoch: u64, index: u32) -> Result<Self, VideoError> {
        let kid = kid(epoch, index);
        Ok(Self {
            key: Key::derive(base_key, kid)?,
            kid,
            ctr: 0,
        })
    }

    /// The SFrame ciphertext of `frame`; `metadata` is authenticated but not sent.
    pub fn encrypt(&mut self, metadata: &[u8], frame: &[u8]) -> Result<Vec<u8>, VideoError> {
        let ctr = self.ctr;
        self.ctr = ctr
            .checked_add(1)
            .ok_or_else(|| VideoError::EncryptionError("counter exhausted".into()))?;
        let mut out = header(self.kid, ctr);
        let aad = [out.as_slice(), metadata].concat();
        let sealed = self
            .key
            .cipher
            .encrypt(
                &self.key.nonce(ctr),
                Payload {
                    msg: frame,
                    aad: &aad,
                },
            )
            .map_err(|_| VideoError::EncryptionError("encryption failed".into()))?;
        out.extend_from_slice(&sealed);
        Ok(out)
    }
}

// The highest accepted counter and a bit per counter below it: bit n is `top - n`.
#[derive(Default)]
struct Window {
    top: Option<u64>,
    seen: u64,
}

impl Window {
    fn fresh(&self, ctr: u64) -> bool {
        match self.top {
            Some(top) if ctr <= top => {
                top - ctr < REPLAY_WINDOW && self.seen & (1 << (top - ctr)) == 0
            }
            _ => true,
        }
    }

    fn accept(&mut self, ctr: u64) {
        match self.top {
            Some(top) if ctr <= top => self.seen |= 1 << (top - ctr),
            _ => {
                let shift = self.top.map_or(REPLAY_WINDOW, |top| ctr - top);
                self.seen = u32::try_from(shift)
                    .ok()
                    .and_then(|shift| self.seen.checked_shl(shift))
                    .unwrap_or(0)
                    | 1;
                self.top = Some(ctr);
            }
        }
    }
}

/// Incoming frames from the other members, by KID.
#[derive(Default)]
pub struct Receiver {
    keys: HashMap<u64, (Key, Window)>,
}

impl Receiver {
    /// Adds the keys of members `indices` in MLS `epoch`, whose exporter gave `base_key`, and
    /// drops those of the older epoch with the same low-order bits.
    pub fn add_epoch(
        &mut self,
        base_key: &[u8],
        epoch: u64,
        indices: &[u32],
    ) -> Result<(), VideoError> {
        let mask = (1 << EPOCH_BITS) - 1;
        self.keys.retain(|kid, _| kid & mask != epoch & mask);
        for &index in indices {
            let kid = kid(epoch, index);
            self.keys
                .insert(kid, (Key::derive(base_key, kid)?, Window::default()));
        }
        Ok(())
    }

    /// The frame inside `ciphertext`. Fails on an unknown KID, a changed byte, other `metadata`
    /// or a replayed counter.
    pub fn decrypt(&mut self, metadata: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, VideoError> {
        let refuse = |why: &str| VideoError::EncryptionError(why.into());
        let (kid, ctr, length) = parse(ciphertext).ok_or_else(|| refuse("bad header"))?;
        let (key, window) = self
            .keys
            .get_mut(&kid)
            .ok_or_else(|| refuse("unknown key"))?;
        if !window.fresh(ctr) {
            return Err(refuse("replayed frame"));
        }
        let (header, sealed) = ciphertext.split_at(length);
        let aad = [header, metadata].concat();
        let frame = key
            .cipher
            .decrypt(
                &key.nonce(ctr),
                Payload {
                    msg: sealed,
                    aad: &aad,
                },
            )
            .map_err(|_| refuse("frame does not decrypt"))?;
        window.accept(ctr);
        Ok(frame)
    }
}

// The SFrame header: a config byte, then KID and CTR in their fewest big-endian bytes if over 7.
fn header(kid: u64, ctr: u64) -> Vec<u8> {
    let mut out = vec![0];
    let field = |value: u64, out: &mut Vec<u8>| -> u8 {
        if value < 8 {
            return value as u8;
        }
        let skip = value.leading_zeros() as usize / 8;
        out.extend_from_slice(&value.to_be_bytes()[skip..]);
        0b1000 | (7 - skip) as u8
    };
    let kid = field(kid, &mut out);
    let ctr = field(ctr, &mut out);
    out[0] = (kid << 4) | ctr;
    out
}

// KID, CTR and header length, or `None` if the data ends inside the header.
fn parse(data: &[u8]) -> Option<(u64, u64, usize)> {
    let config = *data.first()?;
    let mut at = 1;
    let mut field = |bits: u8| -> Option<u64> {
        if bits & 0b1000 == 0 {
            return Some(bits.into());
        }
        let bytes = data.get(at..at + usize::from(bits & 0b111) + 1)?;
        at += bytes.len();
        Some(
            bytes
                .iter()
                .fold(0, |value, &byte| (value << 8) | u64::from(byte)),
        )
    };
    let kid = field(config >> 4)?;
    let ctr = field(config & 0b1111)?;
    Some((kid, ctr, at))
}

#[cfg(test)]
mod tests;
