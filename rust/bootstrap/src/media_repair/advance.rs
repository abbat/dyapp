//! One repair transition at a time; only the active blob holds payload buffers.
use super::{Blob, Job, Repair, Stage};
use crate::media::MediaStore;
use crate::replication::{media_decode, media_encode, MediaManifestExt, MEDIA_OBJECT_BYTES};
use crate::service::Service;
use libp2p::PeerId;
use std::collections::HashSet;

pub(super) enum Step {
    Retry,
    Wait,
    Ready,
}

impl Blob {
    fn local(&mut self, media: &MediaStore) {
        self.data = media.get(&self.hash).ok().flatten().filter(|data| {
            data.len() <= MEDIA_OBJECT_BYTES && dyapp_identity::sha256(data).as_slice() == self.hash
        });
        self.whole = self.data.is_some() || !self.sources.is_empty();
        if self.whole {
            return;
        }
        let Some(manifest) = &self.manifest else {
            return;
        };
        for index in 0..manifest.shard_hashes.len() {
            if let Ok(Some(data)) = media.shard(&self.hash, index) {
                if manifest.accepts(index, &data) {
                    self.shards.insert(index, data);
                }
            }
        }
    }

    fn recoverable(&self) -> bool {
        let available = self
            .shard_sources
            .keys()
            .map(|index| *index as usize)
            .chain(self.shards.keys().copied())
            .collect::<HashSet<_>>()
            .len();
        self.whole
            || self.manifest.as_ref().is_some_and(|manifest| {
                manifest
                    .data_shards()
                    .is_some_and(|count| available >= count)
            })
    }

    fn retain_gaps(&mut self) {
        self.gaps.retain(|(_, index)| match index {
            None => true,
            Some(index) => {
                !self.whole
                    && self
                        .manifest
                        .as_ref()
                        .is_some_and(|manifest| (*index as usize) < manifest.shard_hashes.len())
            }
        });
    }

    fn shard_jobs(&mut self, blob: usize, me: PeerId) -> Vec<Job> {
        let mut jobs = Vec::new();
        for (index, sources) in &mut self.shard_sources {
            if self.shards.contains_key(&(*index as usize)) {
                continue;
            }
            while let Some(peer) = sources.pop() {
                if peer != me {
                    jobs.push(Job::ShardGet(blob, *index, peer));
                    break;
                }
            }
        }
        jobs
    }

    fn decode_failed(&self, media: &MediaStore, error: &impl std::fmt::Display) {
        tracing::warn!(%error, "media repair decode failed");
        let _ = media.discard_sharded(&self.hash).inspect_err(|error| {
            tracing::error!(%error, "invalid media repair metadata not discarded");
        });
    }

    fn decode(&self, media: &MediaStore) -> Option<Vec<u8>> {
        let shards = self
            .shards
            .iter()
            .map(|(index, data)| (*index, data.clone()))
            .collect::<Vec<_>>();
        match media_decode(self.manifest.as_ref().unwrap(), &shards) {
            Ok(data) => Some(data),
            Err(error) => {
                self.decode_failed(media, &error);
                None
            }
        }
    }

    fn rebuild(&mut self, media: &MediaStore) -> bool {
        let Some(data) = self.decode(media) else {
            self.missing = true;
            return false;
        };
        self.data = Some(data);
        match media_encode(self.data.as_ref().unwrap()) {
            Ok((manifest, shards)) => {
                self.manifest = Some(manifest);
                self.encoded = shards;
                true
            }
            Err(error) => {
                tracing::error!(%error, "media repair encode failed");
                self.missing = true;
                false
            }
        }
    }
}

impl Repair {
    pub(super) fn inventory_pending(&mut self) -> bool {
        if self.inventoried {
            return false;
        }
        self.inventoried = true;
        for peer in self.groups.keys().copied().collect::<Vec<_>>() {
            self.issue(Job::Have(peer));
        }
        !self.jobs.is_empty()
    }
    pub(super) fn prepare_active(&mut self, service: &Service) -> bool {
        let blob = &mut self.blobs[self.active];
        blob.local(service.media.as_ref().unwrap());
        if !blob.recoverable() {
            self.clear_active();
            return false;
        }
        blob.missing = false;
        blob.retain_gaps();
        if blob.gaps.is_empty() {
            self.clear_active();
            return false;
        }
        self.stage = Stage::Fetch;
        true
    }

    pub(super) fn fetch_active(&mut self, service: &Service, me: PeerId) -> Step {
        let blob = &self.blobs[self.active];
        if blob.whole && blob.data.is_none() {
            return self.fetch_whole(me);
        }
        if !blob.whole {
            return self.fetch_shards(service, me);
        }
        Step::Ready
    }

    fn fetch_whole(&mut self, me: PeerId) -> Step {
        let blob = &mut self.blobs[self.active];
        if let Some(peer) = blob.sources.pop() {
            if peer != me {
                self.issue(Job::Get(self.active, peer));
                return Step::Wait;
            }
        }
        if !blob.sources.is_empty() {
            return Step::Retry;
        }
        blob.missing = true;
        self.clear_active();
        Step::Retry
    }

    fn fetch_shards(&mut self, service: &Service, me: PeerId) -> Step {
        let blob = &mut self.blobs[self.active];
        let count = blob.manifest.as_ref().unwrap().data_shards().unwrap();
        if blob.shards.len() < count {
            let jobs = blob.shard_jobs(self.active, me);
            if !jobs.is_empty() {
                for job in jobs {
                    self.issue(job);
                }
                return Step::Wait;
            }
            blob.missing = true;
        } else if blob.rebuild(service.media.as_ref().unwrap()) {
            return Step::Ready;
        }
        self.clear_active();
        Step::Retry
    }

    pub(super) fn store_active(&mut self) {
        let jobs = self.blobs[self.active]
            .gaps
            .iter()
            .map(|(peer, index)| Job::Put(self.active, *index, *peer))
            .collect::<Vec<_>>();
        self.stage = Stage::Store;
        for job in jobs {
            self.issue(job);
        }
    }
}
