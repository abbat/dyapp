//! Fresh requests retain the selected hash even when the selected history row was cleared.
use super::*;
use crate::chat::pack_control::PackRequest;
use crate::store::{sticker_packs::exchange::download::PackDownloadJob, Error as StoreError};

impl Session {
    /// Explicitly request a new descriptor, retiring the old answer with the new MLS/outbox commit.
    pub fn stage_fresh_sticker_pack_request(
        &mut self,
        job: &PackDownloadJob,
        seal_key: &[u8; 32],
    ) -> Result<(PendingPack, Vec<PreparedEnvelope>), Error> {
        let group = self.pair_group(&job.peer).ok_or(Error::UnknownGroup)?;
        let current = self
            .store
            .pending_sticker_pack(&job.peer, &job.pack_id)?
            .ok_or(StoreError::InboxConflict)?;
        if current.token != job.token || current.sticker_hash != job.sticker_hash {
            return Err(StoreError::InboxConflict.into());
        }
        let body = Body::PackRequest(PackRequest {
            pack_id: job.pack_id,
        });
        self.stage_pack_control(&group, &body, seal_key, |store, sealed, writes| {
            store.stage_fresh_sticker_pack_request(sealed, writes, &group, job)
        })
    }

    /// Deliver a fresh request after its new generation and exact encrypted writes are durable.
    pub async fn send_fresh_sticker_pack_request(
        &mut self,
        nodes: &mut [Node],
        job: &PackDownloadJob,
        seal_key: &[u8; 32],
    ) -> Result<PendingPack, Error> {
        let (pending, writes) = self.stage_fresh_sticker_pack_request(job, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(pending)
    }
}
