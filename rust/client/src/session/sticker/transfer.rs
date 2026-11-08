//! Whole-pack responses use the same immutable blob and upload transport as photos.
use super::{Error, Node, Session};
use crate::attachment;
use crate::chat::pack_control::PackAnswer;
use crate::store::{
    sticker_packs::{self, uploads::UploadPlan},
    Conversation, Error as StoreError,
};
use dyapp_identity::Identity;

const RESPONSE_BATCH: usize = 4;

impl Session {
    /// Process a bounded response batch, retaining failed uploads for deadline-based retry.
    /// Only completed outbox staging advances cooldown; later mailbox errors use normal retry.
    ///
    /// # Errors
    ///
    /// Returns an error if the identity is not the store owner, reading or updating
    /// job state fails, encryption or upload reservation fails, descriptor checks
    /// fail, or staging, recovery, or non-client delivery fails. Earlier jobs may
    /// already be committed; client upload/delivery errors use durable retry.
    pub async fn respond_sticker_pack_requests(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        seal_key: &[u8; 32],
        stop: impl Fn() -> bool,
    ) -> Result<usize, Error> {
        if identity.peer_id() != self.store.owner_peer_id() {
            return Err(crate::Error::Reply("sticker owner mismatch").into());
        }
        let jobs = self.store.due_sticker_pack_responses(crate::now() * 1000)?;
        let mut staged = 0;
        for job in jobs.into_iter().take(RESPONSE_BATCH) {
            if stop() {
                break;
            }
            if self.partner(&job.group).as_deref() != Some(job.peer.as_str())
                || self.store.blocked(&job.peer)?
                || self.store.conversation(&job.peer)? != Conversation::Open
                || !self.store.shared_sticker_pack(&job.peer, &job.pack_id)?
            {
                self.store.retire_sticker_pack_response(&job)?;
                continue;
            }
            let pack = match self.store.saved_sticker_pack(seal_key, &job.pack_id) {
                Ok(Some(pack)) => pack,
                Ok(None) | Err(sticker_packs::Error::Invalid) => {
                    self.store.retire_sticker_pack_response(&job)?;
                    continue;
                }
                Err(_) => {
                    self.store
                        .defer_sticker_pack_response(&job, crate::now() * 1000)?;
                    continue;
                }
            };
            let bytes = pack.encode();
            let (mut candidate, _) = attachment::seal(&bytes)?;
            candidate.mime = "application/octet-stream".into();
            let mut descriptor = match self
                .store
                .plan_sticker_pack_upload(&candidate, crate::now())?
            {
                UploadPlan::Reuse(descriptor) => {
                    super::upload::ciphertext(&descriptor, &bytes)?;
                    descriptor
                }
                UploadPlan::Upload(pending) => {
                    let blob = super::upload::ciphertext(&pending.attachment, &bytes)?;
                    let started = crate::now();
                    match super::super::attachment::upload(
                        nodes,
                        identity,
                        &pending.attachment,
                        &blob,
                    )
                    .await
                    {
                        Ok(()) => {}
                        Err(Error::Client(_)) => {
                            self.store
                                .defer_sticker_pack_response(&job, crate::now() * 1000)?;
                            continue;
                        }
                        Err(error) => return Err(error),
                    }
                    self.store.finish_sticker_pack_upload(&pending, started)?;
                    pending.attachment
                }
                UploadPlan::Fallback => {
                    return Err(crate::Error::Reply("invalid pack upload plan").into())
                }
            };
            if stop() {
                break;
            }
            descriptor.release_secret = [0; 32];
            let answer = PackAnswer {
                pack_id: job.pack_id,
                attachment: descriptor,
            };
            let writes = match self.stage_sticker_pack_response(&job, &answer, seal_key) {
                Ok(writes) => writes,
                Err(error) => {
                    self.recover(seal_key)?;
                    match error {
                        Error::Store(
                            StoreError::StickerPackUnavailable
                            | StoreError::BlockedPeer
                            | StoreError::ChatClosed
                            | StoreError::GroupMismatch,
                        ) => {
                            self.store.retire_sticker_pack_response(&job)?;
                        }
                        Error::Store(StoreError::InboxConflict) => {
                            self.store
                                .defer_sticker_pack_response(&job, crate::now() * 1000)?;
                        }
                        error => return Err(error),
                    }
                    continue;
                }
            };
            staged += 1;
            match self.send_staged(nodes, writes, seal_key).await {
                Ok(()) | Err(Error::Client(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(staged)
    }
}
