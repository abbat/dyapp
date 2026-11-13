//! Foreign readback replaces the old baseline, never edits committed during the request.
use super::{
    merge, profile_retention, Error, ProfilePublication, ProfilePublicationOutcome, Store,
};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use rusqlite::{Transaction, TransactionBehavior};

impl Store {
    /// Project verified same-owner remote content while retaining post-prepare local edits.
    /// The caller performs verification and photo fetching before acquiring this writer.
    pub(crate) fn commit_profile_import(
        &self,
        identity: &Identity,
        attempt: &ProfilePublication,
        imported: &Profile,
    ) -> Result<ProfilePublicationOutcome, Error> {
        if identity.peer_id() != self.owner_peer_id || attempt.owner != self.owner_peer_id {
            return Err(Error::WrongIdentity);
        }
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let current = self.profile_draft()?;
        let floor = current
            .as_ref()
            .map_or(0, |draft| draft.version)
            .max(self.own_profile_seen()?)
            .max(self.known_search_version()?);
        if (attempt.baseline.is_some() && current.is_none()) || floor > imported.version {
            return Err(Error::ProfileCompletionSuperseded);
        }
        let initial = Profile::default();
        let baseline = attempt.baseline.as_ref().unwrap_or(&initial);
        let mut draft = current.map_or_else(
            || imported.clone(),
            |current| merge::merge(baseline, &current, imported),
        );
        let hidden = self.profile_hidden()?;
        let queued = if hidden && !imported.deleted {
            draft.version = imported
                .version
                .checked_add(1)
                .ok_or(Error::ProfileVersionExhausted)?;
            Profile::tombstone(draft.version, draft.pow_nonce, draft.pow_bits)
        } else {
            imported.clone()
        };
        let hashes: Vec<_> = imported
            .photos
            .iter()
            .flat_map(|photo| photo.full.iter().chain([&photo.thumbnail]).cloned())
            .collect();
        self.save_profile_draft(&draft)?;
        self.hold_search_profile_in_tx(&queued.sign(identity))?;
        profile_retention::retain(&transaction, &hashes, &draft)?;
        transaction.commit()?;
        Ok(ProfilePublicationOutcome::snapshot(imported, draft, hidden))
    }
}
