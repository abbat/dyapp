use crate::{
    replication::{MediaManifestExt, MEDIA_OBJECT_BYTES},
    service::Service,
};
#[path = "media_repair/advance.rs"]
mod advance;
#[path = "media_repair/ancestry.rs"]
pub(crate) mod ancestry;
#[path = "media_repair/inventory.rs"]
mod inventory;
#[path = "media_repair/requests.rs"]
mod requests;
#[cfg(test)]
use crate::replication::media_encode;
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::{proto, REPLICAS};
use libp2p::PeerId;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) const INTERVAL: Duration = Duration::from_secs(3600);
pub(crate) const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TRACKED: usize = 4096;
const WINDOW: usize = 256;
pub(crate) const MAX_ITEMS: usize = WINDOW * 11;

#[derive(Default)]
pub(crate) struct Schedule {
    cursors: HashMap<Vec<u8>, (usize, Instant)>,
    recent: HashMap<Vec<u8>, (Instant, bool)>,
}

impl Schedule {
    pub(crate) fn select(
        &mut self,
        owner: Vec<u8>,
        hashes: &[Vec<u8>],
        now: Instant,
    ) -> Vec<Vec<u8>> {
        self.recent.retain(|_, (at, _)| now < *at + INTERVAL);
        self.cursors.retain(|_, (_, at)| now < *at + INTERVAL);
        if hashes.is_empty()
            || (!self.cursors.contains_key(&owner) && self.cursors.len() >= MAX_TRACKED)
        {
            return Vec::new();
        }
        let (cursor, seen) = self.cursors.entry(owner).or_insert((0, now));
        *seen = now;
        // Only ever used modulo the length, so a cursor past a shrunken list still works.
        let start = *cursor;
        let count = hashes.len().min(WINDOW);
        *cursor = (start + count) % hashes.len();
        let mut selected = Vec::new();
        for offset in 0..count {
            let hash = &hashes[(start + offset) % hashes.len()];
            if !self.recent.contains_key(hash) && self.recent.len() < MAX_TRACKED {
                self.recent.insert(hash.clone(), (now, true));
                selected.push(hash.clone());
            }
        }
        selected
    }

    pub(crate) fn result(&self, hash: &[u8], now: Instant) -> Option<bool> {
        self.recent
            .get(hash)
            .filter(|(at, _)| now < *at + INTERVAL)
            .map(|(_, missing)| *missing)
    }

    pub(crate) fn forget(&mut self, hash: &[u8]) {
        self.recent.remove(hash);
    }

    pub(crate) fn finish(&mut self, hash: &[u8], missing: bool) {
        if let Some((_, value)) = self.recent.get_mut(hash) {
            *value = missing;
        }
    }
}

#[derive(Clone)]
enum Job {
    History(PeerId),
    Have(PeerId),
    Get(usize, PeerId),
    ShardGet(usize, u32, PeerId),
    Put(usize, Option<u32>, PeerId),
}

impl Job {
    fn peer(&self) -> PeerId {
        match self {
            Self::History(p)
            | Self::Have(p)
            | Self::Get(_, p)
            | Self::ShardGet(_, _, p)
            | Self::Put(_, _, p) => *p,
        }
    }
}

struct Blob {
    hash: Vec<u8>,
    manifest: Option<proto::MediaManifest>,
    whole: bool,
    sources: Vec<PeerId>,
    shard_sources: HashMap<u32, Vec<PeerId>>,
    gaps: Vec<(PeerId, Option<u32>)>,
    data: Option<Vec<u8>>,
    shards: HashMap<usize, Vec<u8>>,
    encoded: Vec<Vec<u8>>,
    missing: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Inventory,
    Fetch,
    Store,
}

/// Metadata for one keep; only the currently repaired blob owns payload buffers.
pub(crate) struct Repair {
    record: Arc<SignedRecord>,
    history: Option<ancestry::History>,
    blobs: Vec<Blob>,
    groups: HashMap<PeerId, Vec<proto::MediaPart>>,
    pub(crate) lookups: usize,
    inventoried: bool,
    unconfirmed: bool,
    active: usize,
    stage: Stage,
    jobs: HashMap<usize, Job>,
    queued: Vec<(PeerId, usize)>,
    next_job: usize,
    pub(crate) deadline: Instant,
}

impl Repair {
    pub(crate) fn new(
        service: &Service,
        record: SignedRecord,
        hashes: Vec<Vec<u8>>,
        now: Instant,
    ) -> crate::Result<Self> {
        let media = service.media.as_ref().unwrap();
        let mut blobs = Vec::new();
        for hash in hashes {
            let whole = media
                .get(&hash)?
                .is_some_and(|d| dyapp_identity::sha256(&d).as_slice() == hash);
            let manifest = media
                .manifest(&hash)?
                .filter(|m| m.hash == hash && m.data_shards().is_some());
            blobs.push(Blob {
                hash,
                manifest,
                whole,
                sources: Vec::new(),
                shard_sources: HashMap::new(),
                gaps: Vec::new(),
                data: None,
                shards: HashMap::new(),
                encoded: Vec::new(),
                missing: true,
            });
        }
        let history = ancestry::History::from_record(&record)?;
        let mut repair = Self {
            record: Arc::new(record),
            history,
            blobs,
            groups: HashMap::new(),
            lookups: 0,
            inventoried: false,
            unconfirmed: false,
            active: 0,
            stage: Stage::Inventory,
            jobs: HashMap::new(),
            queued: Vec::new(),
            next_job: 0,
            deadline: now + TIMEOUT,
        };
        repair.lookups = (0..repair.blobs.len()).map(|i| repair.keys(i).len()).sum();
        Ok(repair)
    }

    pub(crate) fn hashes(&self) -> impl Iterator<Item = &[u8]> {
        self.blobs.iter().map(|b| b.hash.as_slice())
    }

    pub(crate) fn keys(&self, blob: usize) -> Vec<u8> {
        let b = &self.blobs[blob];
        let count = if b.whole {
            usize::from(REPLICAS)
        } else {
            b.manifest
                .as_ref()
                .map_or(10, |m| m.shard_hashes.len())
                .max(usize::from(REPLICAS))
        };
        let Ok(count) = u8::try_from(count) else {
            return Vec::new();
        };
        (0..count).collect()
    }

    pub(crate) fn holder(&mut self, blob: usize, index: u8, peer: Option<PeerId>) {
        self.lookups = self.lookups.saturating_sub(1);
        let Some(peer) = peer else {
            self.unconfirmed = true;
            return;
        };
        let b = &self.blobs[blob];
        let items = self.groups.entry(peer).or_default();
        let mut add = |index| {
            let part = proto::MediaPart {
                hash: b.hash.clone(),
                index,
            };
            if !items.contains(&part) {
                items.push(part);
            }
        };
        if index < REPLICAS {
            add(None);
        }
        if !b.whole
            && b.manifest
                .as_ref()
                .is_none_or(|m| usize::from(index) < m.shard_hashes.len())
        {
            add(Some(u32::from(index)));
        }
    }

    fn issue(&mut self, job: Job) {
        let id = self.next_job;
        self.next_job += 1;
        self.queued.push((job.peer(), id));
        self.jobs.insert(id, job);
    }

    pub(crate) fn queued(&mut self) -> Vec<(PeerId, usize)> {
        std::mem::take(&mut self.queued)
    }

    pub(crate) fn has_job(&self, id: usize) -> bool {
        self.jobs.contains_key(&id)
    }

    pub(crate) fn request(
        &mut self,
        id: usize,
        service: &Service,
    ) -> Option<(proto::MediaRequest, u64)> {
        use proto::media_request::Request;
        if let Some(Job::History(peer)) = self.jobs.get(&id) {
            return self.history_request(*peer, service);
        }
        let (request, reply) = match self.jobs.get(&id)? {
            Job::History(_) => return None,
            Job::Have(peer) => {
                let items = self.groups.get(peer)?.clone();
                // Reserve both partition lists and one bounded manifest per requested hash.
                let reply = items.len() as u64 * 512 + 1024;
                (Request::Have(proto::MediaHave { items }), reply)
            }
            Job::Get(blob, _) => (
                Request::Get(proto::GetMedia {
                    hash: self.blobs[*blob].hash.clone(),
                    ..Default::default()
                }),
                (MEDIA_OBJECT_BYTES + 1024) as u64,
            ),
            Job::ShardGet(blob, index, _) => (
                Request::ShardGet(proto::MediaShardGet {
                    hash: self.blobs[*blob].hash.clone(),
                    index: *index,
                }),
                (self.blobs[*blob].manifest.as_ref()?.shard_size()? + 1024) as u64,
            ),
            Job::Put(blob, index, _) => (self.write(*blob, *index)?, 1024),
        };
        Some((
            proto::MediaRequest {
                request: Some(request),
            },
            reply,
        ))
    }

    pub(crate) fn response(&mut self, id: usize, response: Option<proto::MediaResponse>) {
        self.response_at(id, response, Instant::now());
    }

    pub(crate) fn response_at(
        &mut self,
        id: usize,
        response: Option<proto::MediaResponse>,
        now: Instant,
    ) {
        let Some(job) = self.jobs.remove(&id) else {
            return;
        };
        let response = response.filter(|r| r.status == i32::from(proto::Status::Ok));
        match job {
            Job::History(peer) => self.history_response(peer, response, now),
            Job::Have(peer) => self.inventory_response(peer, response),
            Job::Get(blob, _) => {
                if let Some(data) = response.map(|r| r.data).filter(|d| {
                    d.len() <= MEDIA_OBJECT_BYTES
                        && dyapp_identity::sha256(d).as_slice() == self.blobs[blob].hash
                }) {
                    self.blobs[blob].data = Some(data);
                    self.blobs[blob].whole = true;
                }
            }
            Job::ShardGet(blob, index, _) => {
                let b = &mut self.blobs[blob];
                if let Some(data) = response.map(|r| r.data).filter(|d| {
                    b.manifest
                        .as_ref()
                        .is_some_and(|m| m.accepts(index as usize, d))
                }) {
                    b.shards.insert(index as usize, data);
                }
            }
            Job::Put(..) => {
                let expected = proto::MediaResponse {
                    status: proto::Status::Ok.into(),
                    ..Default::default()
                };
                self.unconfirmed |= response.as_ref() != Some(&expected);
            }
        }
    }

    fn inventory(&mut self, peer: PeerId, inventory: &proto::MediaInventory) {
        let requested = &self.groups[&peer];
        if !inventory::valid(requested, inventory) {
            self.unconfirmed = true;
            return;
        }
        self.unconfirmed |= inventory.present.len() + inventory.missing.len() != requested.len();
        for blob in &mut self.blobs {
            blob.record_inventory(peer, inventory);
        }
    }

    pub(crate) fn advance(&mut self, service: &Service, me: PeerId) {
        if self.lookups != 0 || !self.jobs.is_empty() {
            return;
        }
        if self.history_pending() || self.inventory_pending() {
            return;
        }
        while self.active < self.blobs.len() {
            if self.stage == Stage::Store {
                self.clear_active();
                continue;
            }
            if self.stage == Stage::Inventory && !self.prepare_active(service) {
                continue;
            }
            match self.fetch_active(service, me) {
                advance::Step::Retry => continue,
                advance::Step::Wait => return,
                advance::Step::Ready => self.store_active(),
            }
            if !self.jobs.is_empty() {
                return;
            }
        }
    }

    fn clear_active(&mut self) {
        let b = &mut self.blobs[self.active];
        b.data = None;
        b.shards.clear();
        b.encoded.clear();
        self.active += 1;
        self.stage = Stage::Inventory;
    }

    pub(crate) fn done(&self) -> bool {
        self.active == self.blobs.len()
    }

    /// A completed transfer requires holder coverage and successful write acknowledgments.
    /// Source availability alone still controls the independent client reupload result.
    pub(crate) fn transferred(&self) -> bool {
        self.done()
            && self.lookups == 0
            && self.jobs.is_empty()
            && !self.unconfirmed
            && self.results().all(|(_, missing)| !missing)
    }

    pub(crate) fn results(&self) -> impl Iterator<Item = (&[u8], bool)> {
        self.blobs
            .iter()
            .enumerate()
            .map(|(i, b)| (b.hash.as_slice(), b.missing || i >= self.active))
    }
}

#[cfg(test)]
#[path = "media_repair/schedule.rs"]
mod schedule_tests;

#[cfg(test)]
mod tests;
