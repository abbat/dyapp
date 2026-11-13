//! Explicit owner confirmation for ambiguous old search withdrawals; never device enrollment.
use super::{search_projection::Head, search_publish::State, Error, Store};
use dyapp_identity::{sha256, Identity, SignedRecord};
use dyapp_profile::Profile;
use rusqlite::{params, Transaction, TransactionBehavior};
use std::sync::OnceLock;

#[derive(PartialEq)]
struct Snapshot {
    draft: Option<Profile>,
    hidden: bool,
    queue: State,
    head: Option<Head>,
    seen: u64,
    listed: u64,
    link: [u8; 32],
}

/// Opaque owner-bound review of one hidden search head; holds no key, session or operation lease.
/// Preparation never signs or writes; only explicit confirmation can replace the reviewed head.
pub struct SearchWithdrawalRecovery {
    owner: String,
    database: String,
    baseline: Snapshot,
    version: u64,
    confirmed: OnceLock<SignedRecord>,
}

impl SearchWithdrawalRecovery {
    fn draft(&self) -> Result<Profile, Error> {
        let mut draft = self
            .baseline
            .draft
            .clone()
            .ok_or(Error::SearchWithdrawalReviewRequired)?;
        if let Some(record) = &self.baseline.queue.pending {
            let pending = dyapp_profile::verify_search(record)
                .map_err(|_| Error::InvalidSetting)?
                .profile;
            if pending.pow_bits > draft.pow_bits {
                (draft.pow_nonce, draft.pow_bits) = (pending.pow_nonce, pending.pow_bits);
            }
        }
        draft.version = self.version;
        Ok(draft)
    }
}

impl Store {
    // Enrollment survives link completion, so even a link finished during review invalidates it.
    fn withdrawal_recovery_context(&self) -> Result<[u8; 32], Error> {
        let mut query = self.db.prepare(
            "SELECT `key`, `format_version`, `value` FROM `settings`
             WHERE `key` IN (?1, ?2, ?3, ?4) ORDER BY `key`",
        )?;
        let rows = query.query_map(
            params![
                "link_pending",
                "linked_device",
                "link_state_import",
                "link_enrollment"
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )?;
        let mut bytes = Vec::new();
        for row in rows {
            let (key, format, value) = row?;
            bytes.extend(sha256(key.as_bytes()));
            bytes.extend(format.to_be_bytes());
            bytes.extend(sha256(&value));
        }
        Ok(sha256(&bytes))
    }

    fn withdrawal_recovery_snapshot(&self) -> Result<Snapshot, Error> {
        let (queue, head) = self.search_projection()?;
        Ok(Snapshot {
            draft: self.profile_draft()?,
            hidden: self.profile_hidden()?,
            queue,
            head,
            seen: self.own_profile_seen()?,
            listed: self.device_listed()?,
            link: self.withdrawal_recovery_context()?,
        })
    }

    /// Review an ambiguous hidden search head without changing state or signing a new record.
    /// Known ordinary publication and purpose-separated withdrawal need no recovery.
    ///
    /// # Errors
    /// Returns invalid/missing draft, version exhaustion, pending link or storage errors.
    pub fn prepare_search_withdrawal_recovery(
        &self,
    ) -> Result<Option<SearchWithdrawalRecovery>, Error> {
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let baseline = self.withdrawal_recovery_snapshot()?;
        if !baseline.head.is_some_and(Head::needs_withdrawal_recovery) {
            return Ok(None);
        }
        if !baseline.hidden {
            return Err(Error::SearchWithdrawalReviewRequired);
        }
        if self.link_pending()?.is_some() {
            return Err(Error::LinkStateReviewRequired);
        }
        let draft = baseline
            .draft
            .as_ref()
            .ok_or(Error::SearchWithdrawalReviewRequired)?;
        let version = draft
            .version
            .max(baseline.head.map_or(0, Head::version))
            .max(baseline.seen)
            .max(baseline.listed)
            .checked_add(1)
            .ok_or(Error::ProfileVersionExhausted)?;
        transaction.commit()?;
        Ok(Some(SearchWithdrawalRecovery {
            owner: self.owner_peer_id().into(),
            database: self.db.path().ok_or(Error::InvalidSetting)?.into(),
            baseline,
            version,
            confirmed: OnceLock::new(),
        }))
    }

    /// Confirm the reviewed hidden state with the original owner key, creating only search authority.
    /// A successful same-plan retry returns current draft state without advancing another version.
    ///
    /// # Errors
    /// Returns owner mismatch, changed review, invalid state or storage errors. Writes are atomic.
    pub fn confirm_search_withdrawal_recovery(
        &self,
        identity: &Identity,
        plan: &SearchWithdrawalRecovery,
    ) -> Result<Profile, Error> {
        if identity.peer_id() != self.owner_peer_id() || plan.owner != self.owner_peer_id() {
            return Err(Error::WrongIdentity);
        }
        if self.db.path() != Some(plan.database.as_str()) {
            return Err(Error::SearchWithdrawalRecoveryChanged);
        }
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let current = self.withdrawal_recovery_snapshot()?;
        if !current.hidden || current.link != plan.baseline.link {
            return Err(Error::SearchWithdrawalRecoveryChanged);
        }
        let draft = plan.draft()?;
        let record =
            Profile::search_tombstone(plan.version, draft.pow_nonce, draft.pow_bits).sign(identity);
        let draft = if current
            .draft
            .as_ref()
            .is_some_and(|draft| draft.version >= plan.version)
            && self.search_profile_known(&record)?
        {
            current.draft.ok_or(Error::SearchWithdrawalReviewRequired)?
        } else {
            if current != plan.baseline {
                return Err(Error::SearchWithdrawalRecoveryChanged);
            }
            self.save_profile_draft(&draft)?;
            self.hold_search_profile_in_tx(&record)?;
            draft
        };
        transaction.commit()?;
        plan.confirmed.get_or_init(|| record);
        Ok(draft)
    }

    /// The exact explicitly confirmed withdrawal when it is still current and due.
    /// Preparation alone grants no retry authority; no signing occurs here.
    ///
    /// # Errors
    /// Returns owner mismatch, changed review context, or malformed/storage state.
    pub fn due_search_withdrawal_recovery(
        &self,
        plan: &SearchWithdrawalRecovery,
        now: u64,
    ) -> Result<Option<SignedRecord>, Error> {
        if plan.owner != self.owner_peer_id() {
            return Err(Error::WrongIdentity);
        }
        if self.db.path() != Some(plan.database.as_str()) {
            return Err(Error::SearchWithdrawalRecoveryChanged);
        }
        let Some(record) = plan.confirmed.get() else {
            return Ok(None);
        };
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        if !self.profile_hidden()?
            || self.withdrawal_recovery_context()? != plan.baseline.link
            || !self.search_profile_known(record)?
        {
            return Err(Error::SearchWithdrawalRecoveryChanged);
        }
        let due = self.due_search_profile(now)?;
        if due.as_ref().is_some_and(|pending| pending != record) {
            return Err(Error::SearchWithdrawalRecoveryChanged);
        }
        transaction.commit()?;
        Ok(due)
    }
}

#[cfg(test)]
mod tests;
