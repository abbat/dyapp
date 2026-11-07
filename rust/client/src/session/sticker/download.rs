//! Download only admitted pack answers; publish through the current request's writer guard.
use super::{Error, Node, Session};
use crate::{
    attachment,
    sticker_pack::{self, Pack},
    store::{
        sticker_packs::{
            exchange::download::{PackDownloadFailure as Failure, PackDownloadJob},
            Error as PackError,
        },
        Conversation, Error as StoreError,
    },
};

impl Session {
    /// Complete at most four admitted downloads, preserving typed failures for scheduled retry.
    /// A stop cannot interrupt an attachment request already awaiting its transport deadline.
    pub async fn download_sticker_packs(
        &mut self,
        nodes: &mut [Node],
        seal_key: &[u8; 32],
        stop: impl Fn() -> bool,
    ) -> Result<usize, Error> {
        let jobs = self.store.due_sticker_pack_downloads(crate::now() * 1000)?;
        let mut completed = 0;
        for job in jobs.into_iter().take(4) {
            if stop() {
                break;
            }
            if self.store.blocked(&job.peer)?
                || self.store.conversation(&job.peer)? != Conversation::Open
            {
                self.store
                    .finish_sticker_pack(&job.peer, &job.pack_id, &job.token)?;
                continue;
            }
            let Some(group) = self.pair_group(&job.peer) else {
                self.defer_pack_download(&job, Failure::Fetch)?;
                continue;
            };
            let Some(answer) =
                self.store
                    .sticker_pack_answer(&group, &job.peer, &job.pack_id, &job.token)?
            else {
                continue;
            };
            match self.store.saved_sticker_pack(seal_key, &job.pack_id) {
                Ok(Some(pack)) => {
                    completed +=
                        usize::from(self.retain_downloaded_pack(&group, &job, &pack, seal_key)?);
                    continue;
                }
                Ok(None) => {}
                Err(_) => {
                    self.defer_pack_download(&job, Failure::Storage)?;
                    continue;
                }
            }
            let fetched = attachment::fetch(nodes, &answer.attachment).await;
            if stop() {
                break;
            }
            let bytes = match fetched {
                Ok(bytes) => bytes,
                Err(_) => {
                    self.defer_pack_download(&job, Failure::Fetch)?;
                    continue;
                }
            };
            completed += usize::from(self.complete_pack_download(&group, &job, &bytes, seal_key)?);
        }
        Ok(completed)
    }

    fn defer_pack_download(&self, job: &PackDownloadJob, failure: Failure) -> Result<(), Error> {
        self.store
            .defer_sticker_pack_download(job, failure, crate::now() * 1000)?;
        Ok(())
    }

    fn complete_pack_download(
        &mut self,
        group: &[u8],
        job: &PackDownloadJob,
        bytes: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<bool, Error> {
        let pack = match sticker_pack::from_peer(bytes, &job.pack_id) {
            Ok(pack) => pack,
            Err(_) => {
                self.defer_pack_download(job, Failure::Invalid)?;
                return Ok(false);
            }
        };
        self.retain_downloaded_pack(group, job, &pack, seal_key)
    }

    fn retain_downloaded_pack(
        &mut self,
        group: &[u8],
        job: &PackDownloadJob,
        pack: &Pack,
        seal_key: &[u8; 32],
    ) -> Result<bool, Error> {
        if !pack
            .stickers()
            .iter()
            .any(|entry| entry.sha256 == job.sticker_hash)
        {
            self.defer_pack_download(job, Failure::Invalid)?;
            return Ok(false);
        }
        match self
            .store
            .save_requested_sticker_pack(seal_key, group, job, pack)
        {
            Ok(_) => return Ok(true),
            Err(PackError::Request(
                StoreError::BlockedPeer
                | StoreError::ChatClosed
                | StoreError::GroupMismatch
                | StoreError::InboxConflict,
            )) => {
                self.store
                    .finish_sticker_pack(&job.peer, &job.pack_id, &job.token)?;
            }
            Err(PackError::Quota) => self.defer_pack_download(job, Failure::Quota)?,
            Err(PackError::Invalid) => self.defer_pack_download(job, Failure::Invalid)?,
            Err(_) => self.defer_pack_download(job, Failure::Storage)?,
        }
        Ok(false)
    }
}
