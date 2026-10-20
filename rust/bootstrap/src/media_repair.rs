use crate::{
    replication::{media_decode, media_encode, MediaManifestExt, MEDIA_OBJECT_BYTES},
    service::Service,
};
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::{proto, REPLICAS};
use libp2p::PeerId;
use std::{
    collections::{HashMap, HashSet},
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
        let start = *cursor % hashes.len();
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

    pub(crate) fn finish(&mut self, hash: &[u8], missing: bool) {
        if let Some((_, value)) = self.recent.get_mut(hash) {
            *value = missing;
        }
    }
}

#[derive(Clone)]
enum Job {
    Have(PeerId),
    Get(usize, PeerId),
    ShardGet(usize, u32, PeerId),
    Put(usize, Option<u32>, PeerId),
}

impl Job {
    fn peer(&self) -> PeerId {
        match self {
            Self::Have(p) | Self::Get(_, p) | Self::ShardGet(_, _, p) | Self::Put(_, _, p) => *p,
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
    blobs: Vec<Blob>,
    groups: HashMap<PeerId, Vec<proto::MediaPart>>,
    pub(crate) lookups: usize,
    inventoried: bool,
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
        let mut repair = Self {
            record: Arc::new(record),
            blobs,
            groups: HashMap::new(),
            lookups: 0,
            inventoried: false,
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
        (0..count as u8).collect()
    }

    pub(crate) fn holder(&mut self, blob: usize, index: u8, peer: Option<PeerId>) {
        self.lookups = self.lookups.saturating_sub(1);
        let Some(peer) = peer else { return };
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

    pub(crate) fn request(&self, id: usize) -> Option<(proto::MediaRequest, u64)> {
        use proto::media_request::Request;
        let (request, reply) = match self.jobs.get(&id)? {
            Job::Have(peer) => {
                let items = self.groups.get(peer)?.clone();
                // Reserve both partition lists and one bounded manifest per requested hash.
                let reply = items.len() as u64 * 512 + 1024;
                (Request::Have(proto::MediaHave { items }), reply)
            }
            Job::Get(blob, _) => (
                Request::Get(proto::GetMedia {
                    hash: self.blobs[*blob].hash.clone(),
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
            Job::Put(blob, index, _) => {
                let b = &self.blobs[*blob];
                let authorization = Some(proto::media_replica_put::Authorization::Keep(
                    (*self.record).clone(),
                ));
                let replica = proto::MediaReplicaPut {
                    data: if b.whole {
                        b.data.as_ref()?.clone()
                    } else {
                        Vec::new()
                    },
                    manifest: if b.whole { None } else { b.manifest.clone() },
                    authorization,
                };
                let request = match index {
                    None => Request::ReplicaPut(replica),
                    Some(index) => Request::ShardPut(proto::MediaShardPut {
                        replica: Some(replica),
                        index: *index,
                        data: b.encoded.get(*index as usize)?.clone(),
                    }),
                };
                (request, 1024)
            }
        };
        Some((
            proto::MediaRequest {
                request: Some(request),
            },
            reply,
        ))
    }

    pub(crate) fn response(&mut self, id: usize, response: Option<proto::MediaResponse>) {
        let Some(job) = self.jobs.remove(&id) else {
            return;
        };
        let response = response.filter(|r| r.status == i32::from(proto::Status::Ok));
        match job {
            Job::Have(peer) => {
                if let Some(inventory) = response.and_then(|r| r.inventory) {
                    self.inventory(peer, inventory);
                }
            }
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
            Job::Put(..) => {}
        }
    }

    fn inventory(&mut self, peer: PeerId, inventory: proto::MediaInventory) {
        let requested = &self.groups[&peer];
        let mut checked = HashSet::new();
        for part in inventory.present.iter().chain(&inventory.missing) {
            if !requested.contains(part) || !checked.insert((part.hash.clone(), part.index)) {
                return;
            }
        }
        if inventory.whole.len() > WINDOW
            || inventory.manifests.len() > WINDOW
            || inventory
                .whole
                .iter()
                .any(|h| !requested.iter().any(|p| p.hash == *h))
            || inventory
                .manifests
                .iter()
                .any(|m| m.data_shards().is_none() || !requested.iter().any(|p| p.hash == m.hash))
        {
            return;
        }
        for b in &mut self.blobs {
            if inventory.whole.contains(&b.hash) && !b.sources.contains(&peer) {
                b.sources.push(peer);
            }
            let manifest = inventory.manifests.iter().find(|m| m.hash == b.hash);
            if b.manifest.is_none() {
                b.manifest = manifest.cloned();
            }
            for p in inventory.present.iter().filter(|p| p.hash == b.hash) {
                if let Some(index) = p.index {
                    if manifest.is_some_and(|m| {
                        Some(m) == b.manifest.as_ref() && (index as usize) < m.shard_hashes.len()
                    }) {
                        let sources = b.shard_sources.entry(index).or_default();
                        if !sources.contains(&peer) {
                            sources.push(peer);
                        }
                    }
                }
            }
            for p in inventory.missing.iter().filter(|p| p.hash == b.hash) {
                if !b.gaps.contains(&(peer, p.index)) {
                    b.gaps.push((peer, p.index));
                }
            }
        }
    }

    pub(crate) fn advance(&mut self, service: &Service, me: PeerId) {
        if self.lookups != 0 || !self.jobs.is_empty() {
            return;
        }
        if !self.inventoried {
            self.inventoried = true;
            for peer in self.groups.keys().copied().collect::<Vec<_>>() {
                self.issue(Job::Have(peer));
            }
            if !self.jobs.is_empty() {
                return;
            }
        }
        while self.active < self.blobs.len() {
            let i = self.active;
            if self.stage == Stage::Store {
                self.clear_active();
                continue;
            }
            let b = &mut self.blobs[i];
            if self.stage == Stage::Inventory {
                let media = service.media.as_ref().unwrap();
                b.data = media.get(&b.hash).ok().flatten().filter(|d| {
                    d.len() <= MEDIA_OBJECT_BYTES && dyapp_identity::sha256(d).as_slice() == b.hash
                });
                b.whole = b.data.is_some() || !b.sources.is_empty();
                if !b.whole {
                    if let Some(m) = &b.manifest {
                        for index in 0..m.shard_hashes.len() {
                            if let Ok(Some(data)) = media.shard(&b.hash, index) {
                                if m.accepts(index, &data) {
                                    b.shards.insert(index, data);
                                }
                            }
                        }
                    }
                }
                let available = b
                    .shard_sources
                    .keys()
                    .map(|i| *i as usize)
                    .chain(b.shards.keys().copied())
                    .collect::<HashSet<_>>()
                    .len();
                let recoverable = b.whole
                    || b.manifest
                        .as_ref()
                        .is_some_and(|m| available >= m.data_shards().unwrap());
                if !recoverable {
                    self.clear_active();
                    continue;
                }
                b.missing = false;
                b.gaps.retain(|(_, index)| match index {
                    None => true,
                    Some(index) => {
                        !b.whole
                            && b.manifest
                                .as_ref()
                                .is_some_and(|m| (*index as usize) < m.shard_hashes.len())
                    }
                });
                if b.gaps.is_empty() {
                    self.clear_active();
                    continue;
                }
                self.stage = Stage::Fetch;
            }
            let b = &mut self.blobs[i];
            if b.whole && b.data.is_none() {
                if let Some(peer) = b.sources.pop() {
                    if peer != me {
                        self.issue(Job::Get(i, peer));
                        return;
                    }
                }
                if !b.sources.is_empty() {
                    continue;
                }
                b.missing = true;
                self.clear_active();
                continue;
            }
            if !b.whole {
                let m = b.manifest.as_ref().unwrap();
                if b.shards.len() < m.data_shards().unwrap() {
                    let mut fetch = Vec::new();
                    for (index, sources) in &mut b.shard_sources {
                        if b.shards.contains_key(&(*index as usize)) {
                            continue;
                        }
                        while let Some(peer) = sources.pop() {
                            if peer != me {
                                fetch.push(Job::ShardGet(i, *index, peer));
                                break;
                            }
                        }
                    }
                    if !fetch.is_empty() {
                        for job in fetch {
                            self.issue(job);
                        }
                        return;
                    }
                    b.missing = true;
                    self.clear_active();
                    continue;
                }
                let shards = b
                    .shards
                    .iter()
                    .map(|(i, d)| (*i, d.clone()))
                    .collect::<Vec<_>>();
                match media_decode(m, &shards) {
                    Ok(data) => b.data = Some(data),
                    Err(error) => {
                        tracing::warn!(%error, "media repair decode failed");
                        if let Err(error) = service.media.as_ref().unwrap().discard_sharded(&b.hash)
                        {
                            tracing::error!(%error, "invalid media repair metadata not discarded");
                        }
                        b.missing = true;
                        self.clear_active();
                        continue;
                    }
                }
                match media_encode(b.data.as_ref().unwrap()) {
                    Ok((manifest, shards)) => {
                        b.manifest = Some(manifest);
                        b.encoded = shards;
                    }
                    Err(error) => {
                        tracing::error!(%error, "media repair encode failed");
                        b.missing = true;
                        self.clear_active();
                        continue;
                    }
                }
            }
            let jobs = b
                .gaps
                .iter()
                .map(|(peer, index)| Job::Put(i, *index, *peer))
                .collect::<Vec<_>>();
            self.stage = Stage::Store;
            for job in jobs {
                self.issue(job);
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
