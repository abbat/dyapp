//! Serialize request proof and retirement with saved-file quota and publication.
use super::PackDownloadJob;
use crate::{
    sticker_pack::Pack,
    store::{
        sticker_packs::{self, Error},
        Store,
    },
};
use rusqlite::params;

impl Store {
    /// Save a validated requested pack and retire only its current, still permitted generation.
    /// A rename followed by failed SQLite commit retains intent; retry validates the saved file.
    pub fn save_requested_sticker_pack(
        &mut self,
        secret: &[u8; 32],
        group: &[u8],
        job: &PackDownloadJob,
        pack: &Pack,
    ) -> Result<bool, Error> {
        self.save_requested_pack_with_limit(secret, group, job, pack, sticker_packs::SAVED_BYTES)
    }
    fn save_requested_pack_with_limit(
        &mut self,
        secret: &[u8; 32],
        group: &[u8],
        job: &PackDownloadJob,
        pack: &Pack,
        limit: u64,
    ) -> Result<bool, Error> {
        let owner = self.owner_peer_id.clone();
        self.save_pack_with_guard(secret, pack, limit, |tx| {
            super::super::ready(tx, &owner, group, &job.peer)?;
            let current = super::super::pending(tx, &job.peer, &job.pack_id)?
                .ok_or(crate::store::Error::InboxConflict)?;
            if current.token != job.token || current.sticker_hash != job.sticker_hash
                || pack.id() != &job.pack_id
                || !pack.stickers().iter().any(|sticker| sticker.sha256 == job.sticker_hash)
            {
                return Err(crate::store::Error::InboxConflict.into());
            }
            tx.execute(
                "DELETE FROM `sticker_pack_requests` WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `request_token` = ?3",
                params![&job.peer, job.pack_id.as_slice(), job.token.as_slice()])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
