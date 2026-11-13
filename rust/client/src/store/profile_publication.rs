//! Prepared own-publication snapshots and one atomic local completion.
use super::{profile_retention, Error, Store};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use rusqlite::{Transaction, TransactionBehavior};

mod import;
#[cfg(test)]
#[path = "profile_publication/import_tests.rs"]
mod import_tests;
mod merge;
mod receipt;

/// Owner-bound local baseline and mutable request; prepare before any network await.
pub struct ProfilePublication {
    owner: String,
    baseline: Option<Profile>,
    hidden: bool,
    request: Profile,
}

impl ProfilePublication {
    /// The request to validate, enrich with this device, then publish.
    #[must_use]
    pub fn draft(&self) -> &Profile {
        &self.request
    }

    /// Modify the request before passing the immutable prepared attempt to publication.
    pub fn draft_mut(&mut self) -> &mut Profile {
        &mut self.request
    }

    /// Whether a device-only relist should keep this prepared profile hidden.
    #[must_use]
    pub fn hidden(&self) -> bool {
        self.hidden
    }
}

/// Exact accepted output and the separately committed local draft.
#[derive(Debug)]
pub struct ProfilePublicationOutcome {
    /// The exact ordinary profile verified at the node, never overlaid with pending edits.
    pub published: Profile,
    /// Current local fields, including edits not present in published output.
    pub draft: Profile,
    /// Whether the local profile remains hidden from search.
    pub hidden: bool,
    /// Local content differs from the accepted profile, excluding a hidden-version increment.
    pub pending: bool,
}

impl ProfilePublicationOutcome {
    fn snapshot(published: &Profile, draft: Profile, hidden: bool) -> Self {
        let mut content = draft.clone();
        content.version = published.version;
        Self {
            published: published.clone(),
            pending: content != *published,
            draft,
            hidden,
        }
    }
}

impl Store {
    /// Read current draft/visibility after follow-up network awaits, retaining exact readback.
    ///
    /// # Errors
    /// Returns owner mismatch, missing/invalid persisted draft or database errors.
    pub fn profile_publication_state(
        &self,
        identity: &Identity,
        published: &Profile,
    ) -> Result<ProfilePublicationOutcome, Error> {
        let snapshot = self.prepare_profile_publication(identity)?;
        let hidden = snapshot.hidden;
        let draft = snapshot
            .baseline
            .ok_or(Error::ProfileCompletionSuperseded)?;
        Ok(ProfilePublicationOutcome::snapshot(
            published, draft, hidden,
        ))
    }

    /// Read draft and visibility from one snapshot, before publication or device enrichment.
    ///
    /// # Errors
    /// Returns owner mismatch, invalid persisted state or database errors.
    pub fn prepare_profile_publication(
        &self,
        identity: &Identity,
    ) -> Result<ProfilePublication, Error> {
        if identity.peer_id() != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let baseline = self.profile_draft()?;
        let hidden = self.profile_hidden()?;
        snapshot.commit()?;
        Ok(ProfilePublication {
            owner: self.owner_peer_id.clone(),
            request: baseline.clone().unwrap_or_default(),
            baseline,
            hidden,
        })
    }

    /// Atomically project an exact own readback while retaining post-prepare local edits.
    ///
    /// The caller verifies signed readback first. Network and proof search stay outside this
    /// transaction. A remote success can precede a refused/failed local completion.
    ///
    /// # Errors
    /// Returns owner/readback mismatch, exhaustion, supersession, search fork or storage errors.
    /// Draft, queue, visibility and blob pruning roll back together.
    pub fn commit_profile_publication(
        &self,
        identity: &Identity,
        attempt: &ProfilePublication,
        published: &Profile,
        stay_hidden: bool,
    ) -> Result<ProfilePublicationOutcome, Error> {
        if self.profile_publication_record(identity, attempt)? != *published {
            return Err(Error::InvalidSetting);
        }
        let signed = published.sign(identity);
        let hashes: Vec<_> = published
            .photos
            .iter()
            .flat_map(|photo| photo.full.iter().chain([&photo.thumbnail]).cloned())
            .collect();
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let current = self.profile_draft()?;
        let known = self.search_profile_known(&signed)?;
        self.admit_publication(attempt, current.as_ref(), published.version, known)?;
        let initial = Profile::default();
        let baseline = attempt.baseline.as_ref().unwrap_or(&initial);
        let mut draft = current.map_or_else(
            || published.clone(),
            |current| merge::merge(baseline, &current, published),
        );
        let queued = if stay_hidden {
            draft.version = draft
                .version
                .checked_add(1)
                .ok_or(Error::ProfileVersionExhausted)?;
            Profile::tombstone(draft.version, draft.pow_nonce, draft.pow_bits)
        } else {
            published.clone()
        };
        self.save_profile_draft(&draft)?;
        self.hold_search_profile_in_tx(&queued.sign(identity))?;
        self.set_profile_hidden(stay_hidden)?;
        profile_retention::retain(&transaction, &hashes, &draft)?;
        self.remember_profile_completion(identity, published, &queued)?;
        transaction.commit()?;
        Ok(ProfilePublicationOutcome::snapshot(
            published,
            draft,
            stay_hidden,
        ))
    }

    /// Freeze and validate the exact next record before any network request.
    pub(crate) fn profile_publication_record(
        &self,
        identity: &Identity,
        attempt: &ProfilePublication,
    ) -> Result<Profile, Error> {
        if identity.peer_id() != self.owner_peer_id || attempt.owner != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let mut expected = attempt.request.clone();
        expected.version = expected
            .version
            .checked_add(1)
            .ok_or(Error::ProfileVersionExhausted)?;
        if expected.deleted {
            return Err(Error::InvalidSetting);
        }
        Ok(expected)
    }

    // Caller owns the immediate writer, including both snapshot reads and the final projection.
    fn admit_publication(
        &self,
        attempt: &ProfilePublication,
        current: Option<&Profile>,
        version: u64,
        known: bool,
    ) -> Result<(), Error> {
        let missing = attempt.baseline.is_some() && current.is_none();
        let local_version = current.map_or(0, |draft| draft.version);
        let floor = local_version
            .max(self.own_profile_seen()?)
            .max(self.known_search_version()?);
        let changed_visibility = self.profile_hidden()? != attempt.hidden;
        if missing
            || floor > version
            || (!known && (local_version == version || changed_visibility))
        {
            return Err(Error::ProfileCompletionSuperseded);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "profile_publication_tests.rs"]
mod tests;
