//! Proof search cursors are not proofs: never pair a resume nonce with an earlier claim.
use super::{profile_draft, Error, Store};
use dyapp_identity::Identity;
use rusqlite::{params, Connection, OptionalExtension};

/// A core-computed step, bound to its identity and target. Only this module creates one.
pub struct ProfileWork {
    owner: String,
    target: u32,
    nonce: u64,
    proof_bits: Option<u32>,
}

/// Authoritative proof/cursor state after a step or a read.
pub struct ProfileWorkResult {
    /// A committed proof meets this target, possibly completed by another worker.
    pub solved: bool,
    /// The valid proof nonce when solved, otherwise the next per-target search nonce.
    pub next_nonce: u64,
}

fn key(bits: u32) -> String {
    format!("profile_work:{bits}")
}

fn cursor(db: &Connection, bits: u32) -> Result<Option<u64>, Error> {
    let stored: Option<(i64, Vec<u8>)> = db
        .query_row(
            "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
            [key(bits)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((version, value)) = stored else {
        return Ok(None);
    };
    if version != 1 {
        return Err(Error::InvalidSetting);
    }
    let bytes: [u8; 8] = value.try_into().map_err(|_| Error::InvalidSetting)?;
    Ok(Some(u64::from_be_bytes(bytes)))
}

impl Store {
    /// Read a target's proof or resume cursor without computing or acquiring writer ownership.
    ///
    /// # Errors
    /// Returns draft, cursor decoding or database failures.
    pub fn profile_work_status(&self, bits: u32) -> Result<ProfileWorkResult, Error> {
        let draft = self.profile_draft()?.unwrap_or_default();
        let solved = draft.pow_bits >= bits;
        let next_nonce = if solved {
            draft.pow_nonce
        } else {
            cursor(&self.db, bits)?.unwrap_or(draft.pow_nonce)
        };
        Ok(ProfileWorkResult { solved, next_nonce })
    }

    /// Compute a bounded step outside writer ownership; keep the result through actual completion.
    ///
    /// # Errors
    /// Returns owner mismatch, draft/cursor decoding or database failures.
    pub fn prepare_profile_work(
        &self,
        identity: &Identity,
        bits: u32,
        tries: u64,
    ) -> Result<ProfileWork, Error> {
        let owner = identity.peer_id();
        if owner != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let draft = self.profile_draft()?.unwrap_or_default();
        let (nonce, proof_bits) = if draft.pow_bits >= bits {
            (draft.pow_nonce, Some(draft.pow_bits))
        } else {
            let start = cursor(&self.db, bits)?.unwrap_or(draft.pow_nonce);
            match dyapp_profile::solve(&identity.public_key(), bits, start, tries) {
                Ok(nonce) => (nonce, Some(bits)),
                Err(next) => (next, None),
            }
        };
        Ok(ProfileWork {
            owner,
            target: bits,
            nonce,
            proof_bits,
        })
    }

    /// Merge only proof/cursor progress into the latest draft under a short writer transaction.
    ///
    /// No hashing occurs here. A weaker or pending late step cannot downgrade a committed proof.
    ///
    /// # Errors
    /// Returns owner mismatch, malformed state or database errors; failure rolls back the cursor
    /// and draft together.
    pub fn commit_profile_work(&mut self, work: &ProfileWork) -> Result<ProfileWorkResult, Error> {
        if work.owner != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let mut next_nonce = work.nonce;
        let draft = profile_draft::transform(&mut self.db, |db, draft| {
            if let Some(bits) = work.proof_bits {
                if bits > draft.pow_bits {
                    (draft.pow_nonce, draft.pow_bits) = (work.nonce, bits);
                }
            }
            if draft.pow_bits < work.target {
                next_nonce = cursor(db, work.target)?
                    .unwrap_or(draft.pow_nonce)
                    .max(work.nonce);
                db.execute(
                    "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
                     ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
                    params![key(work.target), next_nonce.to_be_bytes().to_vec()],
                )?;
            }
            Ok(())
        })?;
        let solved = draft.pow_bits >= work.target;
        Ok(ProfileWorkResult {
            solved,
            next_nonce: if solved { draft.pow_nonce } else { next_nonce },
        })
    }
}

#[cfg(test)]
#[path = "profile_work_tests.rs"]
mod tests;
