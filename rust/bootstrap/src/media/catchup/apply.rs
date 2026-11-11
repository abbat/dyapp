//! Commit only the final projection after reverse proofs reach live ownership.
use super::{
    lock, owner_of, owner_usage, sha256, storage, storage_error, validate, Limits, MediaStore,
    Message, Result, SignedRecord, TransactionBehavior,
};
use crate::error::BootstrapError;
use crate::media::{keeps, KeepSnapshot};
use rusqlite::Connection;
use std::ops::RangeInclusive;
#[path = "apply/proofs.rs"]
mod proofs;

/// Progress of authenticated history application without disclosing current ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepHistoryProgress {
    /// More reverse proofs are required.
    Pending {
        /// Next revision derived from the supplied proof frontier.
        next_version: u64,
    },
    /// The target is accepted, possibly as an ancestor of the live head.
    Ready {
        /// Next revision available for optional history backfill.
        next_version: u64,
    },
    /// Supplied proofs conflict with this session or accepted ownership.
    Conflict,
}

enum Action {
    Pending,
    Conflict,
    Accept,
    Backfill,
}

fn action(
    db: &Connection,
    owner: &str,
    target: &SignedRecord,
    session: &storage::Session,
    current: &KeepSnapshot,
) -> Result<Action> {
    if let Some(matches) = storage::accepted(db, owner, session.version, target)? {
        if !matches {
            return Ok(Action::Conflict);
        }
        if current.version < session.version {
            return Err(storage_error("accepted target exceeds live head"));
        }
        return Ok(Action::Backfill);
    }
    if current.version >= session.version {
        return Ok(Action::Conflict);
    }
    if current.version == 0 && current.record.is_none() && keeps::owned(db, owner)?.is_empty() {
        return Ok(Action::Accept);
    }
    if current.matches(&session.frontier) {
        return Ok(Action::Accept);
    }
    if current.version > session.frontier.version {
        if let Some(record) = &current.record {
            return Ok(
                if storage::known(db, owner, current.version, record)? == Some(true) {
                    Action::Accept
                } else {
                    Action::Conflict
                },
            );
        }
    }
    Ok(if session.frontier.version <= current.version {
        Action::Conflict
    } else {
        Action::Pending
    })
}

fn project_final(
    db: &Connection,
    owner: &str,
    target: &SignedRecord,
    head: &dyapp_p2p_net::proto::MediaKeepIntent,
    parts: &proofs::Parts,
) -> Result<Vec<Vec<u8>>> {
    let old = keeps::owned(db, owner)?;
    let version = i64::try_from(head.version).map_err(storage_error)?;
    let time = i64::try_from(parts.latest_time(head.time)).map_err(storage_error)?;
    Ok(keeps::project(db, owner, (version, time), &head.hashes, Some(target), old)?.dropped)
}

impl MediaStore {
    /// Apply the final target atomically when its staged ancestry reaches current ownership.
    /// Intermediate lists never change references. Accepted ancestor backfill does not renew
    /// retention. Freshness is checked again at application; errors preserve live state.
    ///
    /// # Errors
    /// Returns invalid signatures/clocks, corrupt proofs, storage, budget or file-removal errors.
    pub fn apply_keep_history(
        &self,
        target: &SignedRecord,
        fresh: RangeInclusive<u64>,
        limits: &Limits,
    ) -> Result<KeepHistoryProgress> {
        let owner = owner_of(target)?;
        let (head, _) = validate(&owner, target, &[], &fresh)?;
        let mut db = lock(&self.db)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let Some(session) = storage::load(&tx, &owner)? else {
            return Ok(KeepHistoryProgress::Pending {
                next_version: head
                    .expected
                    .as_ref()
                    .ok_or_else(|| storage_error("missing target predecessor"))?
                    .version,
            });
        };
        if session.version != head.version || session.hash != sha256(&target.encode_to_vec()) {
            return Ok(KeepHistoryProgress::Conflict);
        }
        let current = keeps::snapshot(&tx, &owner)?;
        let selected = action(&tx, &owner, target, &session, &current)?;
        match selected {
            Action::Pending => {
                return Ok(KeepHistoryProgress::Pending {
                    next_version: session.frontier.version,
                })
            }
            Action::Conflict => {
                storage::discard(&tx, &owner)?;
                tx.commit().map_err(storage_error)?;
                return Ok(KeepHistoryProgress::Conflict);
            }
            Action::Accept | Action::Backfill => {}
        }
        let parts = proofs::Parts::validated(&tx, &owner, target, &session, *fresh.end())?;
        let dropped = if matches!(selected, Action::Accept) {
            project_final(&tx, &owner, target, &head, &parts)?
        } else {
            Vec::new()
        };
        proofs::archive(&tx, &owner)?;
        if owner_usage(&tx, &owner)? > limits.media_per_owner_mb.saturating_mul(1 << 20)
            || Self::used(&tx)? > limits.media_max_mb.saturating_mul(1 << 20)
        {
            return Err(BootstrapError::MediaHistoryFull);
        }
        tx.commit().map_err(storage_error)?;
        self.remove(dropped)?;
        Ok(KeepHistoryProgress::Ready {
            next_version: session.frontier.version,
        })
    }
}
