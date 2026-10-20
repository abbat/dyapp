//! Mailbox write token revocation and device mailbox closing: `revoke` and `replica_revoke`.

use super::*;

/// Token keys one revoke may name.
const MAX_REVOKE_KEYS: usize = 64;

/// How far a revoke outlives its `created_at`: tokens are issued up to 30 days ahead, so a key
/// is revoked to the end of its day and one TTL of replica circulation after it.
const REVOKE_DAYS: i64 = 31;

/// The revoked key that marks a closed mailbox; a token key is never empty.
const CLOSED: &[u8] = &[];

impl Service {
    /// A revoke signed by the mailbox's device key: revokes its token keys and deletes their
    /// envelopes. One signed by the identity key the device accepted as its owner closes the mailbox:
    /// deletes all its envelopes and refuses its puts and fetches. Answers and whether a DENIED
    /// strikes: a forged record strikes, a `created_at` more than an hour ahead does not.
    pub(super) fn revoke(&self, record: &SignedRecord) -> crate::Result<(MailboxResponse, bool)> {
        let Ok((signer, revoke)) =
            owner_request::<proto::Revoke>(record, Domain::MailboxRevoke, |r| {
                r.token_keys.len() <= MAX_REVOKE_KEYS
            })
        else {
            return Ok((status(Status::Denied), true));
        };
        let owned = match &revoke.owner {
            None => revoke.mailbox == signer,
            Some(owner) => owned_by(owner, &record.public_key, &revoke.mailbox),
        };
        if !owned {
            return Ok((status(Status::Denied), true));
        }
        if revoke.token_keys.is_empty() == revoke.owner.is_none()
            || revoke.token_keys.iter().any(|key| key.len() != 32)
        {
            return Ok((status(Status::Invalid), false));
        }
        let now = chrono::Utc::now().timestamp();
        if revoke.created_at > now + 3600 {
            return Ok((status(Status::Denied), false));
        }
        if self.listed(&revoke.mailbox) {
            return Ok((status(Status::Refused), false));
        }
        let expires_at = revoke
            .created_at
            .saturating_add(REVOKE_DAYS * 86_400 + self.retention() as i64 + 3600);
        if expires_at <= now {
            return Ok((status(Status::Stale), false));
        }
        let keys = if revoke.owner.is_some() {
            vec![CLOSED.to_vec()]
        } else {
            revoke.token_keys
        };
        let stored = self
            .store
            .revoke(&revoke.mailbox, record, &keys, expires_at, now)?;
        Ok((
            status(if stored { Status::Ok } else { Status::Full }),
            false,
        ))
    }

    /// Whether the owner closed `mailbox` now.
    pub(super) fn closed(&self, mailbox: &[u8]) -> crate::Result<bool> {
        self.revoked(mailbox, CLOSED)
    }

    /// Whether `key` is revoked for `mailbox` now.
    pub(super) fn revoked(&self, mailbox: &[u8], key: &[u8]) -> crate::Result<bool> {
        self.store
            .is_revoked(mailbox, key, chrono::Utc::now().timestamp())
    }

    /// The unexpired signed revokes of a mailbox, for repair.
    pub fn revokes(&self, mailbox: &[u8]) -> crate::Result<Vec<SignedRecord>> {
        self.store.revokes(mailbox, chrono::Utc::now().timestamp())
    }
}

/// Whether `owner` is signed by the device key behind `mailbox` and names `identity`.
fn owned_by(owner: &SignedRecord, identity: &[u8], mailbox: &[u8]) -> bool {
    owner.payload == identity
        && owner.verify(Domain::DeviceOwner).is_ok()
        && <[u8; 32]>::try_from(owner.public_key.as_slice())
            .is_ok_and(|key| dyapp_identity::key_hash(&key).as_slice() == mailbox)
}
