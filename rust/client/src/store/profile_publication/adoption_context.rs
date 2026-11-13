//! One local review context, including link ownership and monotonic publication progress.
use super::{Error, ProfilePublication, Store};
use crate::store::search_projection::Head;
use dyapp_identity::Identity;
use rusqlite::{Transaction, TransactionBehavior};

pub(crate) struct ProfileAdoptionContext {
    pub(crate) publication: ProfilePublication,
    database: String,
    pending_link: Vec<u8>,
    head: Option<Head>,
    seen: u64,
    listed: u64,
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
        if identity.peer_id() != self.owner_peer_id
            || expected.publication.owner != self.owner_peer_id
        {
            return Err(Error::WrongIdentity);
        }
        let Some(current) = self.prepare_profile_adoption_context(identity)? else {
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

#[cfg(test)]
mod tests;
