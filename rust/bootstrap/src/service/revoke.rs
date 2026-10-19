//! Mailbox write token revocation: `revoke` and `replica_revoke`.

use super::*;

/// Token keys one revoke may name.
const MAX_REVOKE_KEYS: usize = 64;

/// How far a revoke outlives its `created_at`: tokens are issued up to 30 days ahead, so a key
/// is revoked to the end of its day and one TTL of replica circulation after it.
const REVOKE_DAYS: i64 = 31;

impl Service {
    /// A revoke signed by the mailbox's device key: revokes its token keys and deletes their
    /// envelopes. Answers and whether a DENIED strikes: a forged record strikes, a `created_at`
    /// more than an hour ahead does not.
    pub(super) fn revoke(&self, record: &SignedRecord) -> crate::Result<(MailboxResponse, bool)> {
        let Ok((mailbox, revoke)) = owner_request::<proto::Revoke>(record, Domain::MailboxRevoke, |r| {
            r.token_keys.len() <= MAX_REVOKE_KEYS
        }) else {
            return Ok((status(Status::Denied), true));
        };
        if revoke.mailbox != mailbox {
            return Ok((status(Status::Denied), true));
        }
        if revoke.token_keys.is_empty() || revoke.token_keys.iter().any(|key| key.len() != 32) {
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
        let stored =
            self.store
                .revoke(&revoke.mailbox, record, &revoke.token_keys, expires_at, now)?;
        Ok((
            status(if stored { Status::Ok } else { Status::Full }),
            false,
        ))
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
