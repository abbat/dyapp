//! Mailbox write tokens. A contact keeps one generated token key per owner mailbox and UTC day,
//! sealed under the app's 32-byte sealing key, with the WriteToken the owner signed for it. The
//! owner keeps the keys it signed per contact so it can revoke them when it blocks that contact.

use super::{Error, Store};
use crate::PreparedEnvelope;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use dyapp_identity::Identity;
use rusqlite::{params, OptionalExtension};

const SEAL_AAD: &[u8] = b"dyapp/token-key/v1\0";
const NONCE_BYTES: usize = 12;
/// A node accepts a token until one day after its epoch ends, so keep it that long.
const KEEP_DAYS: u32 = 1;

impl Store {
    /// The public token key for `mailbox` on `epoch`, generated and sealed on first use; a retry
    /// returns the same key.
    pub fn token_key(
        &mut self,
        mailbox: &[u8; 32],
        epoch: u32,
        seal_key: &[u8; 32],
    ) -> Result<[u8; 32], Error> {
        if let Some(key) = self.held_key(mailbox, epoch, seal_key)? {
            return Ok(key.public_key());
        }
        let key = Identity::generate();
        let mut nonce = [0; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| Error::Random)?;
        let sealed = Aes256Gcm::new(seal_key.into())
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: key.secret().as_slice(),
                    aad: &aad(mailbox, epoch),
                },
            )
            .map_err(|_| Error::TokenConflict)?;
        self.db.execute(
            "INSERT INTO `held_tokens` (`mailbox`, `epoch`, `sealed_key`) VALUES (?1, ?2, ?3)",
            params![mailbox, epoch, [nonce.as_slice(), &sealed].concat()],
        )?;
        Ok(key.public_key())
    }

    /// Record the owner's signed WriteToken for a held key; the caller has verified it.
    pub fn set_write_token(
        &self,
        mailbox: &[u8; 32],
        epoch: u32,
        token: &[u8],
    ) -> Result<(), Error> {
        let changed = self.db.execute(
            "UPDATE `held_tokens` SET `token` = ?3 WHERE `mailbox` = ?1 AND `epoch` = ?2",
            params![mailbox, epoch, token],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(Error::TokenConflict)
        }
    }

    /// The token key and signed WriteToken that let this profile write to `mailbox` on `epoch`.
    pub fn write_token(
        &self,
        mailbox: &[u8; 32],
        epoch: u32,
        seal_key: &[u8; 32],
    ) -> Result<Option<(Identity, Vec<u8>)>, Error> {
        let token: Option<Option<Vec<u8>>> = self
            .db
            .query_row(
                "SELECT `token` FROM `held_tokens` WHERE `mailbox` = ?1 AND `epoch` = ?2",
                params![mailbox, epoch],
                |row| row.get(0),
            )
            .optional()?;
        let Some(token) = token.flatten() else {
            return Ok(None);
        };
        let key = self
            .held_key(mailbox, epoch, seal_key)?
            .ok_or(Error::TokenConflict)?;
        Ok(Some((key, token)))
    }

    /// The days from `from` on with a WriteToken for `mailbox`, ascending.
    pub fn write_token_epochs(&self, mailbox: &[u8; 32], from: u32) -> Result<Vec<u32>, Error> {
        let mut query = self.db.prepare(
            "SELECT `epoch` FROM `held_tokens`
             WHERE `mailbox` = ?1 AND `epoch` >= ?2 AND `token` IS NOT NULL ORDER BY `epoch`",
        )?;
        let epochs = query
            .query_map(params![mailbox, from], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(epochs)
    }

    /// Remember that this profile signed `token_key` for `contact` to write to `mailbox`.
    pub fn record_issued(
        &self,
        contact: &str,
        mailbox: &[u8; 32],
        token_key: &[u8; 32],
        epoch: u32,
    ) -> Result<(), Error> {
        let stored: String = self.db.query_row(
            "INSERT INTO `issued_tokens` (`mailbox`, `token_key`, `epoch`, `contact`)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO UPDATE SET `contact` = `contact` RETURNING `contact`",
            params![mailbox, token_key, epoch, contact],
            |row| row.get(0),
        )?;
        if stored == contact {
            Ok(())
        } else {
            Err(Error::TokenConflict)
        }
    }

    /// Mark every token issued to `contact` revoked and return the keys still valid on or after
    /// `today`, which the node must be told to refuse.
    pub fn revoke_issued(&self, contact: &str, today: u32) -> Result<Vec<[u8; 32]>, Error> {
        self.db.execute(
            "UPDATE `issued_tokens` SET `revoked` = 1 WHERE `contact` = ?1",
            [contact],
        )?;
        let mut query = self.db.prepare(
            "SELECT DISTINCT `token_key` FROM `issued_tokens`
             WHERE `contact` = ?1 AND `epoch` + ?2 >= ?3 ORDER BY `token_key`",
        )?;
        let keys = query
            .query_map(params![contact, KEEP_DAYS, today], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(keys)
    }

    /// Whether `token_key` was issued to a contact this profile later revoked.
    pub fn issued_revoked(&self, token_key: &[u8; 32]) -> Result<bool, Error> {
        Ok(self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM `issued_tokens` WHERE `token_key` = ?1 AND `revoked` = 1)",
            [token_key],
            |row| row.get(0),
        )?)
    }

    /// Replace pending write `refused`, which a node refused for its token, with `next`, the same
    /// ciphertext under another token or none, in one commit.
    pub fn replace_outbound(
        &mut self,
        refused: &PreparedEnvelope,
        next: &PreparedEnvelope,
    ) -> Result<(), Error> {
        if refused.mailbox() != next.mailbox() {
            return Err(Error::OutboxConflict);
        }
        // An in-place update keeps whether the row waits for a receipt.
        let updated = self.db.execute(
            "UPDATE `outbox` SET `envelope_id` = ?4, `signed_envelope` = ?5, `retry_at_ms` = 0,
             `attempts` = 0
             WHERE `mailbox` = ?1 AND `envelope_id` = ?2 AND `signed_envelope` = ?3",
            params![
                refused.mailbox().as_slice(),
                refused.id().as_slice(),
                refused.as_bytes(),
                next.id().as_slice(),
                next.as_bytes()
            ],
        )?;
        if updated != 1 {
            return Err(Error::OutboxConflict);
        }
        Ok(())
    }

    /// Delete held and issued tokens that no node accepts on `today` any more.
    pub fn prune_tokens(&self, today: u32) -> Result<(), Error> {
        for table in ["held_tokens", "issued_tokens"] {
            self.db.execute(
                &format!("DELETE FROM `{table}` WHERE `epoch` + ?1 < ?2"),
                params![KEEP_DAYS, today],
            )?;
        }
        Ok(())
    }

    pub(crate) fn held_key(
        &self,
        mailbox: &[u8; 32],
        epoch: u32,
        seal_key: &[u8; 32],
    ) -> Result<Option<Identity>, Error> {
        let sealed: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT `sealed_key` FROM `held_tokens` WHERE `mailbox` = ?1 AND `epoch` = ?2",
                params![mailbox, epoch],
                |row| row.get(0),
            )
            .optional()?;
        let Some(sealed) = sealed else {
            return Ok(None);
        };
        let (nonce, ciphertext) = sealed
            .split_at_checked(NONCE_BYTES)
            .ok_or(Error::TokenConflict)?;
        let secret = zeroize::Zeroizing::new(
            Aes256Gcm::new(seal_key.into())
                .decrypt(
                    Nonce::from_slice(nonce),
                    Payload {
                        msg: ciphertext,
                        aad: &aad(mailbox, epoch),
                    },
                )
                .map_err(|_| Error::TokenConflict)?,
        );
        let secret: &[u8; 32] = secret
            .as_slice()
            .try_into()
            .map_err(|_| Error::TokenConflict)?;
        Ok(Some(Identity::from_secret(secret)))
    }
}

/// Binds a sealed key to its row, so a copied ciphertext does not open for another day or mailbox.
fn aad(mailbox: &[u8; 32], epoch: u32) -> Vec<u8> {
    [SEAL_AAD, mailbox, &epoch.to_be_bytes()].concat()
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_db_path;
    use super::super::{Error, Store};
    use crate::prepare_put;
    use dyapp_identity::Identity;

    #[test]
    fn held_key_is_sealed_stable_and_bound_to_its_row() {
        let path = temp_db_path();
        let alice = Identity::generate();
        let (seal, mailbox) = ([7; 32], [1; 32]);
        let mut store = Store::open(&path, &alice).unwrap();
        let key = store.token_key(&mailbox, 100, &seal).unwrap();
        assert_eq!(store.token_key(&mailbox, 100, &seal).unwrap(), key);
        assert_ne!(store.token_key(&mailbox, 101, &seal).unwrap(), key);
        assert!(store.write_token(&mailbox, 100, &seal).unwrap().is_none());
        assert!(matches!(
            store.set_write_token(&[2; 32], 100, b"token"),
            Err(Error::TokenConflict)
        ));
        store.set_write_token(&mailbox, 100, b"token").unwrap();
        drop(store);

        let mut store = Store::open(&path, &alice).unwrap();
        let (held, token) = store.write_token(&mailbox, 100, &seal).unwrap().unwrap();
        assert_eq!((held.public_key(), token.as_slice()), (key, &b"token"[..]));
        assert!(matches!(
            store.write_token(&mailbox, 100, &[8; 32]),
            Err(Error::TokenConflict)
        ));
        let raw: Vec<u8> = store
            .db
            .query_row(
                "SELECT `sealed_key` FROM `held_tokens` WHERE `epoch` = 100",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.windows(32).any(|w| w == held.secret().as_slice()));
        // A ciphertext copied to another day does not open there.
        store
            .db
            .execute(
                "UPDATE `held_tokens` SET `sealed_key` = ?1, `token` = x'00' WHERE `epoch` = 101",
                [&raw],
            )
            .unwrap();
        assert!(matches!(
            store.write_token(&mailbox, 101, &seal),
            Err(Error::TokenConflict)
        ));
        assert!(store.token_key(&mailbox, 101, &seal).is_err());
    }

    #[test]
    fn epochs_list_only_answered_days_and_prune_drops_expired_ones() {
        let alice = Identity::generate();
        let (seal, mailbox) = ([7; 32], [1; 32]);
        let mut store = Store::open(&temp_db_path(), &alice).unwrap();
        for epoch in [9, 10, 11, 12] {
            store.token_key(&mailbox, epoch, &seal).unwrap();
            if epoch != 11 {
                store.set_write_token(&mailbox, epoch, b"t").unwrap();
            }
        }
        assert_eq!(store.write_token_epochs(&mailbox, 10).unwrap(), [10, 12]);
        assert!(store.write_token_epochs(&[2; 32], 0).unwrap().is_empty());
        // Yesterday's token still works for the node's late window.
        store.prune_tokens(11).unwrap();
        assert_eq!(store.write_token_epochs(&mailbox, 0).unwrap(), [10, 12]);
        store.prune_tokens(12).unwrap();
        assert_eq!(store.write_token_epochs(&mailbox, 0).unwrap(), [12]);
    }

    #[test]
    fn revoking_a_contact_returns_its_live_keys_only() {
        let alice = Identity::generate();
        let store = Store::open(&temp_db_path(), &alice).unwrap();
        let (bob, carol) = ("bob", "carol");
        let mailbox = [1; 32];
        store.record_issued(bob, &mailbox, &[3; 32], 8).unwrap();
        store.record_issued(bob, &mailbox, &[4; 32], 10).unwrap();
        store.record_issued(bob, &[2; 32], &[4; 32], 11).unwrap();
        store.record_issued(carol, &mailbox, &[5; 32], 10).unwrap();
        store.record_issued(bob, &mailbox, &[4; 32], 10).unwrap();
        assert!(matches!(
            store.record_issued(carol, &mailbox, &[4; 32], 10),
            Err(Error::TokenConflict)
        ));
        assert!(!store.issued_revoked(&[4; 32]).unwrap());
        assert_eq!(store.revoke_issued(bob, 10).unwrap(), [[4; 32]]);
        assert!(store.issued_revoked(&[3; 32]).unwrap());
        assert!(store.issued_revoked(&[4; 32]).unwrap());
        assert!(!store.issued_revoked(&[5; 32]).unwrap());
        assert!(store.revoke_issued("dave", 10).unwrap().is_empty());
        store.prune_tokens(12).unwrap();
        assert!(!store.issued_revoked(&[3; 32]).unwrap());
        assert!(store.issued_revoked(&[4; 32]).unwrap());
    }

    #[test]
    fn a_refused_write_is_swapped_for_its_replacement_once() {
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let refused = prepare_put(&[1; 32], vec![9]).unwrap();
        store
            .stage_outbound(b"mls", std::slice::from_ref(&refused))
            .unwrap();
        let next = prepare_put(&[1; 32], vec![9]).unwrap();
        store.replace_outbound(&refused, &next).unwrap();
        let due = store.due_outbound(0).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].as_bytes(), next.as_bytes());
        assert!(matches!(
            store.replace_outbound(&refused, &next),
            Err(Error::OutboxConflict)
        ));
        let elsewhere = prepare_put(&[2; 32], vec![9]).unwrap();
        assert!(matches!(
            store.replace_outbound(&next, &elsewhere),
            Err(Error::OutboxConflict)
        ));
        assert_eq!(
            store.due_outbound(0).unwrap()[0].as_bytes(),
            next.as_bytes()
        );
    }
}
