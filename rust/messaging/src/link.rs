//! Seals a link record to the one-use key of a `dyapp-link` code
//! ([ADR 0040](../../../docs/decisions/0040-device-linking.md)).

use crate::mls::{mls, Error};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::crypto::OpenMlsCrypto;
use openmls_traits::random::OpenMlsRand;
use openmls_traits::types::{
    HpkeAeadType, HpkeCiphertext, HpkeConfig, HpkeKdfType, HpkeKemType, HpkePrivateKey,
};
use openmls_traits::OpenMlsProvider;

const CONFIG: HpkeConfig = HpkeConfig(
    HpkeKemType::DhKem25519,
    HpkeKdfType::HkdfSha256,
    HpkeAeadType::ChaCha20Poly1305,
);
const INFO: &[u8] = b"dyapp device link";
const KEY_LEN: usize = 32;

/// One-use X25519 key of a new device. The private half goes with the first record it opens, and
/// whatever fails to open leaves it, so junk from someone who saw the code spends nothing.
pub struct LinkKey {
    private: Option<HpkePrivateKey>,
    public: Vec<u8>,
}

impl LinkKey {
    /// A fresh key.
    pub fn generate() -> Result<Self, Error> {
        let provider = OpenMlsRustCrypto::default();
        let seed: HpkePrivateKey = provider.rand().random_vec(KEY_LEN).map_err(mls)?.into();
        let pair = provider
            .crypto()
            .derive_hpke_keypair(CONFIG, &seed)
            .map_err(mls)?;
        Ok(Self {
            private: Some(pair.private),
            public: pair.public,
        })
    }

    /// The public half, for the code.
    pub fn public(&self) -> &[u8] {
        &self.public
    }

    /// Erases the private key: nothing opens any more.
    pub fn erase(&mut self) {
        self.private = None;
    }

    /// Opens a [`seal`]ed record; the private key is erased on success.
    pub fn open(&mut self, sealed: &[u8]) -> Result<Vec<u8>, Error> {
        let private = self.private.as_ref().ok_or(Error::State)?;
        let (kem_output, ciphertext) = sealed.split_at_checked(KEY_LEN).ok_or(Error::State)?;
        let input = HpkeCiphertext {
            kem_output: kem_output.to_vec().into(),
            ciphertext: ciphertext.to_vec().into(),
        };
        let record = OpenMlsRustCrypto::default()
            .crypto()
            .hpke_open(CONFIG, &input, private, INFO, &[])
            .map_err(|_| Error::State)?;
        self.private = None;
        Ok(record)
    }
}

/// Seals `record` to the public half of a [`LinkKey`].
pub fn seal(public: &[u8], record: &[u8]) -> Result<Vec<u8>, Error> {
    let sealed = OpenMlsRustCrypto::default()
        .crypto()
        .hpke_seal(CONFIG, public, INFO, &[], record)
        .map_err(mls)?;
    Ok([sealed.kem_output.as_slice(), sealed.ciphertext.as_slice()].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_opens_once_and_only_with_its_key() {
        let mut key = LinkKey::generate().unwrap();
        let mut other = LinkKey::generate().unwrap();
        let sealed = seal(key.public(), b"record").unwrap();
        assert!(!sealed.windows(6).any(|part| part == b"record"));

        assert!(other.open(&sealed).is_err());
        let mut damaged = sealed.clone();
        damaged[40] ^= 1;
        assert!(key.open(&damaged).is_err());
        assert!(key.open(&sealed[..31]).is_err());

        assert_eq!(key.open(&sealed).unwrap(), b"record");
        assert!(key.open(&sealed).is_err());
    }

    #[test]
    fn a_malformed_public_key_seals_nothing() {
        assert!(seal(&[1; 5], b"record").is_err());
    }
}
