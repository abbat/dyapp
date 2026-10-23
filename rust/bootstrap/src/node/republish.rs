//! Holder-side republish: hands each held key whose closest nodes changed to the current ones
//! through the owner repair paths (ADR 0009).

use super::*;
use crate::republish::{replica_keys, Batch, Scanner, Store, BATCH};

/// How often the stores are scanned.
pub(super) const REPUBLISH_INTERVAL: Duration = Duration::from_secs(3600);
/// The wait after the first finished Kademlia bootstrap, so the routing table settles first.
pub(super) const REPUBLISH_WARMUP: Duration = Duration::from_secs(600);
const MAX_REPUBLISH: usize = 16;
/// No key is republished while traffic is at this share of the cap or above, in percent.
const REPUBLISH_SHARE: u64 = 50;
const STORES: [Store; 3] = [Store::Mailbox, Store::Profile, Store::Media];

/// A key being handed over: done when nothing is outstanding, committed unless it failed.
struct Handover {
    store: Store,
    key: Vec<u8>,
    fingerprint: u64,
    outstanding: usize,
    failed: bool,
}

#[derive(Default)]
pub(super) struct Republish {
    pub(super) scanner: Scanner,
    ready_at: Option<Instant>,
    jobs: HashMap<u64, Handover>,
    pub(super) queries: HashMap<kad::QueryId, u64>,
    requests: HashMap<OutboundRequestId, u64>,
    /// Media repair → job.
    media: HashMap<u64, u64>,
    next_job: u64,
    next_store: usize,
}

impl Republish {
    /// A Kademlia bootstrap finished; the first one starts the warmup.
    pub(super) fn bootstrapped(&mut self, now: Instant) {
        self.ready_at.get_or_insert(now + REPUBLISH_WARMUP);
    }

    pub(super) fn len(&self) -> usize {
        self.jobs.len()
    }

    fn settle(&mut self, job: Option<u64>, ok: bool) -> Option<u64> {
        let handover = self.jobs.get_mut(&job?)?;
        handover.outstanding -= 1;
        handover.failed |= !ok;
        job
    }

    /// A lookup finished; returns its job, for the requests it starts.
    pub(super) fn query_done(&mut self, id: kad::QueryId, ok: bool) -> Option<u64> {
        let job = self.queries.remove(&id);
        self.settle(job, ok)
    }

    /// A request answered or failed; returns its job, for the requests it starts.
    pub(super) fn answered(&mut self, id: OutboundRequestId, ok: bool) -> Option<u64> {
        let job = self.requests.remove(&id);
        self.settle(job, ok)
    }

    /// Request `id` belongs to `job`, if any.
    pub(super) fn request(&mut self, job: Option<u64>, id: OutboundRequestId) {
        let Some(job) = job else { return };
        if let Some(handover) = self.jobs.get_mut(&job) {
            handover.outstanding += 1;
            self.requests.insert(id, job);
        }
    }

    /// Something of `job` was not sent.
    pub(super) fn fail(&mut self, job: Option<u64>) {
        if let Some(handover) = job.and_then(|job| self.jobs.get_mut(&job)) {
            handover.failed = true;
        }
    }

    pub(super) fn media_done(&mut self, repair: u64, complete: bool) {
        let job = self.media.remove(&repair);
        self.settle(job, complete);
    }

    /// Commits the finished jobs that handed everything over; a failed key stays dirty.
    fn reap(&mut self) {
        let scanner = &mut self.scanner;
        self.jobs.retain(|_, job| {
            if job.outstanding == 0 && !job.failed {
                scanner.commit(job.store, &job.key, job.fingerprint);
            }
            job.outstanding > 0
        });
    }
}

/// Up to `BATCH` keys of `store` after `cursor`, with their replica keys.
fn read(service: &Service, store: Store, cursor: &[u8]) -> crate::Result<Batch> {
    let after = std::str::from_utf8(cursor).unwrap_or_default();
    let owner = |record: &SignedRecord| <[u8; 32]>::try_from(record.public_key.as_slice()).ok();
    Ok(match store {
        Store::Mailbox => service
            .store
            .mailboxes_after(cursor, BATCH)?
            .into_iter()
            .map(|mailbox| {
                let keys = replica_keys(store, std::slice::from_ref(&mailbox));
                (mailbox, keys)
            })
            .collect(),
        Store::Profile => service
            .store
            .profiles_after(after, BATCH)?
            .iter()
            .filter_map(owner)
            .map(|key| {
                let hash = dyapp_identity::key_hash(&key).to_vec();
                (
                    dyapp_identity::peer_id(&key).into_bytes(),
                    replica_keys(store, &[hash]),
                )
            })
            .collect(),
        Store::Media => match &service.media {
            Some(media) => media
                .keeps_after(after, BATCH)?
                .into_iter()
                .map(|(owner, record)| {
                    let hashes = proto::MediaKeep::decode(record.payload.as_slice())
                        .map(|keep| keep.hashes)
                        .unwrap_or_default();
                    (owner.into_bytes(), replica_keys(store, &hashes))
                })
                .collect(),
            None => Vec::new(),
        },
    })
}

impl Replicas {
    /// The hourly scan: queues the held keys whose closest nodes changed, from the routing table
    /// alone. Nothing runs before the bootstrap warmup has passed.
    pub(super) fn scan(&mut self, swarm: &mut Swarm<Behaviour>, service: &Service, now: Instant) {
        if !self.republish.ready_at.is_some_and(|at| now >= at) {
            return;
        }
        for store in STORES {
            let batch = match read(service, store, self.republish.scanner.cursor(store)) {
                Ok(batch) => batch,
                Err(error) => {
                    tracing::error!(%error, ?store, "republish scan failed");
                    continue;
                }
            };
            let wrapped = batch.len() < BATCH as usize;
            let kad = &mut swarm.behaviour_mut().kad;
            let closest = |key: &[u8]| {
                kad.get_closest_local_peers(&kad::KBucketKey::new(key.to_vec()))
                    .take(REPLICAS.into())
                    .map(|peer| *peer.preimage())
                    .collect()
            };
            self.republish.scanner.scan(store, batch, closest, wrapped);
        }
    }

    /// Starts handing over queued keys, round robin over the stores, while fewer than
    /// `MAX_REPUBLISH` are in flight and traffic is under `REPUBLISH_SHARE`. A key its owner
    /// repaired this hour is skipped and stays dirty.
    pub(super) fn republish(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        now: Instant,
    ) {
        self.republish.reap();
        let mut empty = 0;
        while self.republish.jobs.len() < MAX_REPUBLISH
            && empty < STORES.len()
            && service.traffic.second() < REPUBLISH_SHARE
        {
            let store = STORES[self.republish.next_store];
            self.republish.next_store = (self.republish.next_store + 1) % STORES.len();
            let Some((key, fingerprint)) = self.republish.scanner.pop(store) else {
                empty += 1;
                continue;
            };
            empty = 0;
            let job = self.republish.next_job;
            self.republish.next_job = job.wrapping_add(1);
            let mut queries = Vec::new();
            let mut media = false;
            match store {
                Store::Mailbox => {
                    if self.repaired.insert(key.clone()) {
                        for i in 0..REPLICAS {
                            let query = swarm
                                .behaviour_mut()
                                .kad
                                .get_closest_peers(replica_key(&key, i));
                            self.lookups.insert(query, Lookup::Repair(key.clone()));
                            queries.push(query);
                        }
                    }
                }
                Store::Profile => {
                    let peer = std::str::from_utf8(&key).unwrap_or_default();
                    match service.store.get_profile(peer) {
                        Ok(Some(record)) => {
                            queries = self.repair_profile(swarm, service, record, now)
                        }
                        Ok(None) => {}
                        Err(error) => tracing::error!(%error, "profile republish failed"),
                    }
                }
                Store::Media => {
                    let owner = std::str::from_utf8(&key).unwrap_or_default();
                    match service.media.as_ref().map(|media| media.keep_of(owner)) {
                        Some(Ok(Some(record))) => {
                            if let Some(repair) =
                                self.start_media_repair(swarm, service, record, now)
                            {
                                self.republish.media.insert(repair, job);
                                media = true;
                            }
                        }
                        Some(Err(error)) => tracing::error!(%error, "media republish failed"),
                        _ => {}
                    }
                }
            }
            if queries.is_empty() && !media {
                continue;
            }
            for &query in &queries {
                self.republish.queries.insert(query, job);
            }
            self.republish.jobs.insert(
                job,
                Handover {
                    store,
                    key,
                    fingerprint,
                    outstanding: queries.len() + usize::from(media),
                    failed: false,
                },
            );
        }
    }
}
