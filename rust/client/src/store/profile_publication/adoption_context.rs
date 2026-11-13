//! One local review context, including link ownership and monotonic publication progress.
use super::{profile_retention, Error, ProfilePublication, ProfilePublicationOutcome, Store};
use crate::store::search_projection::Head;
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use rusqlite::{Transaction, TransactionBehavior};

pub(crate) struct ProfileAdoptionContext {
    pub(crate) publication: ProfilePublication,
    database: String,
    pending_link: Vec<u8>,
    head: Option<Head>,
    seen: u64,
    listed: u64,
}

impl ProfileAdoptionContext {
    pub(crate) fn pending_link_state(&self) -> &[u8] {
        &self.pending_link
    }
}

impl Store {
    pub(crate) fn prepare_profile_adoption_context(
        &self,
        identity: &Identity,
    ) -> Result<Option<ProfileAdoptionContext>, Error> {
        if identity.peer_id() != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let context = self.profile_adoption_context_in_snapshot()?;
        snapshot.commit()?;
        Ok(context)
    }

    pub(crate) fn profile_adoption_context_current(
        &self,
        identity: &Identity,
        expected: &ProfileAdoptionContext,
    ) -> Result<bool, Error> {
        self.admit_adoption_review_owner(identity, expected)?;
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let matches = self.adoption_review_matches_in_snapshot(expected)?;
        snapshot.commit()?;
        Ok(matches)
    }

    fn admit_adoption_review_owner(
        &self,
        identity: &Identity,
        expected: &ProfileAdoptionContext,
    ) -> Result<(), Error> {
        if identity.peer_id() != self.owner_peer_id
            || expected.publication.owner != self.owner_peer_id
        {
            return Err(Error::WrongIdentity);
        }
        Ok(())
    }

    // The preflight and final writer use exactly the same matching predicate.
    fn adoption_review_matches_in_snapshot(
        &self,
        expected: &ProfileAdoptionContext,
    ) -> Result<bool, Error> {
        let Some(current) = self.profile_adoption_context_in_snapshot()? else {
            return Ok(false);
        };
        Ok(current.database == expected.database
            && current.pending_link == expected.pending_link
            && current.publication.baseline == expected.publication.baseline
            && current.publication.hidden == expected.publication.hidden
            && current.head == expected.head
            && current.seen == expected.seen
            && current.listed == expected.listed)
    }

    pub(crate) fn validate_reviewed_adoption(
        &self,
        identity: &Identity,
        expected: &ProfileAdoptionContext,
        remote: &Profile,
        selected: &Profile,
    ) -> Result<(), Error> {
        self.admit_adoption_review_owner(identity, expected)?;
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        if !self.adoption_review_matches_in_snapshot(expected)? {
            return Err(Error::ProfileCompletionSuperseded);
        }
        review_projection(expected, remote, selected)?;
        snapshot.commit()?;
        Ok(())
    }

    pub(crate) fn commit_reviewed_adoption(
        &self,
        identity: &Identity,
        expected: &ProfileAdoptionContext,
        remote: &Profile,
        selected: &Profile,
    ) -> Result<ProfilePublicationOutcome, Error> {
        self.admit_adoption_review_owner(identity, expected)?;
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if !self.adoption_review_matches_in_snapshot(expected)? {
            return Err(Error::ProfileCompletionSuperseded);
        }
        let (draft, queued) = review_projection(expected, remote, selected)?;
        let hashes: Vec<_> = remote
            .photos
            .iter()
            .flat_map(|photo| photo.full.iter().chain([&photo.thumbnail]).cloned())
            .collect();
        self.save_profile_draft(&draft)?;
        self.hold_search_profile_in_tx(&queued.sign(identity))?;
        profile_retention::retain(&self.db, &hashes, &draft)?;
        // The caller verified this session certificate in the exact remote readback.
        self.note_device_listed(remote.version)?;
        self.remember_profile_completion(identity, remote, &queued)?;
        transaction.commit()?;
        Ok(ProfilePublicationOutcome::snapshot(
            remote,
            draft,
            expected.publication.hidden,
        ))
    }

    // Historical verified membership is factual knowledge, not field consent. Local draft edits
    // may advance after review; the original database, owner and pending operation must still match.
    pub(crate) fn remember_reviewed_membership(
        &self,
        identity: &Identity,
        expected: &ProfileAdoptionContext,
        version: Option<u64>,
    ) -> Result<(), Error> {
        self.admit_adoption_review_owner(identity, expected)?;
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if self.db.path() != Some(expected.database.as_str())
            || self.link_pending()?.as_deref() != Some(expected.pending_link.as_slice())
        {
            return Err(Error::LinkStateChanged);
        }
        if let Some(version) = version {
            self.note_device_listed(version)?;
        }
        transaction.commit()?;
        Ok(())
    }

    // Caller holds one SQLite snapshot. Search ACK/backoff may change without changing its head.
    fn profile_adoption_context_in_snapshot(
        &self,
    ) -> Result<Option<ProfileAdoptionContext>, Error> {
        let publication = self.profile_publication_snapshot()?;
        let Some(pending_link) = self.link_pending()? else {
            return Ok(None);
        };
        Ok(Some(ProfileAdoptionContext {
            publication,
            database: self.db.path().ok_or(Error::InvalidSetting)?.to_owned(),
            pending_link,
            head: self.search_projection()?.1,
            seen: self.own_profile_seen()?,
            listed: self.device_listed()?,
        }))
    }
}

// The owner saves a reviewed local draft; ordinary public content stays exactly as read back.
fn review_projection(
    expected: &ProfileAdoptionContext,
    remote: &Profile,
    selected: &Profile,
) -> Result<(Profile, Profile), Error> {
    if remote.deleted
        || expected.publication.draft().deleted
        || remote.version < expected.seen.max(expected.listed)
    {
        return Err(Error::ProfileCompletionSuperseded);
    }
    if selected.devices != remote.devices
        || selected.device_owners != remote.device_owners
        || selected.deleted
        || selected.version != remote.version
    {
        return Err(Error::InvalidSetting);
    }
    let floor = remote
        .version
        .max(expected.publication.draft().version)
        .max(expected.head.map_or(0, Head::version))
        .max(expected.seen)
        .max(expected.listed);
    let mut draft = selected.clone();
    let queued = if expected.publication.hidden {
        draft.version = floor.checked_add(1).ok_or(Error::ProfileVersionExhausted)?;
        Profile::search_tombstone(draft.version, draft.pow_nonce, draft.pow_bits)
    } else {
        if floor > remote.version {
            return Err(Error::ProfileCompletionSuperseded);
        }
        draft.version = remote.version;
        remote.clone()
    };
    Ok((draft, queued))
}

#[cfg(test)]
mod tests;
