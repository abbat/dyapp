//! Answers node protocol requests (`/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`,
//! `/dyapp/media`) from the stores. The libp2p loop in `dyappd` passes each request here and sends back the reply.

use crate::replication::MediaManifestExt;
use crate::{
    config::{Role, Turn},
    maintenance::RebuildBudget,
    media::{MediaStore, Put},
    pow::PowCheck,
    rate_limit::{Memory, PeerRateLimiter, Reputation, Traffic},
    search::{SearchIndex, SEARCH_FIELDS},
    BootstrapError, BootstrapStore, NodeConfig,
};
use dyapp_identity::{Domain, SignedRecord};
use dyapp_p2p_net::proto::{
    self, mailbox_request, media_request, node_request, profile_request, search_request,
    InventoryRequest, InventoryResponse, MailboxRequest, MailboxResponse, MediaRequest,
    MediaResponse, NodeInfo, NodeRequest, NodeResponse, ProfileRequest, ProfileResponse,
    SearchRequest, SearchResponse, Status, TurnCredentials,
};
use prost::Message;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Envelope payload send limit advertised in node info.
pub const MAX_ENVELOPE_BYTES: usize = 100 * 1024;
/// Network acceptance is raised before the send limit during a rollout.
const ENVELOPE_ACCEPT_BYTES: usize = MAX_ENVELOPE_BYTES;
const PROFILE_SEND_BYTES: usize = dyapp_profile::MAX_PAYLOAD_LEN;
const PROFILE_ACCEPT_BYTES: usize = dyapp_profile::MAX_PAYLOAD_LEN;
/// Fixed network record admission; raise before increasing the client-send default.
pub(crate) const SEARCH_ACCEPT_BYTES: usize = 64 << 10;
/// Stored bytes per device mailbox.
pub const MAX_MAILBOX_BYTES: u64 = 10 * 1024 * 1024;
/// Envelope bytes per fetch reply, leaving 64 KiB for protobuf framing.
const FETCH_BYTES: u64 = 1024 * 1024;
const MAX_FETCH: u32 = 100;
/// Largest media blob, well under the protocol message limit; larger files are split.
pub const MAX_MEDIA_BYTES: usize = crate::replication::MEDIA_MAX_BYTES;
/// Hashes in one media keep.
const MAX_KEEP: usize = 256;
/// Unreleased attachments per owner, and blobs in one: a blob row costs the node without a put.
const MAX_ATTACHMENTS: usize = 256;
const MAX_ATTACH: usize = 16;
/// How far an attach's `created` may run ahead of the node's clock.
const CLOCK_SKEW: u64 = 600;
/// Envelope ids in one repair inventory.
pub const MAX_INVENTORY: usize = 10_000;

/// Shares of `bytes_per_second` (percent) from which media and profile requests are shed; the
/// mailbox is shed only at the limit; repair and search stop with media.
pub const SHED_MEDIA: u64 = 75;
const SHED_PROFILES: u64 = 90;
/// Share of `max_memory_mb` (percent) from which requests that shed by the byte rate are
/// refused; from 100 % every request is dropped unanswered (see `Service::overloaded`).
const SHED_MEMORY: u64 = 80;

/// The remote side of a request: its libp2p peer ID and IP group (see `node::ip_group`).
pub struct Peer {
    pub id: String,
    pub group: String,
}

pub struct Service {
    pub store: BootstrapStore,
    /// Set when the node serves the media role.
    pub media: Option<MediaStore>,
    /// Set when the node serves the search role.
    pub search: Option<Arc<SearchIndex>>,
    /// Connected search nodes: their get and inventory requests are charged to `search_sync`.
    search_peers: Mutex<HashSet<String>>,
    /// Profile proof-of-work results, shared with the blocking pool.
    pub pow: Arc<crate::pow::PowCache>,
    /// Reply bytes per second to search nodes catching up, `maintenance.io_mb_per_s`.
    pub search_sync: Traffic,
    pub rate_limiter: PeerRateLimiter,
    pub media_peers: PeerRateLimiter,
    media_bytes: PeerRateLimiter,
    pub groups: PeerRateLimiter,
    pub senders: PeerRateLimiter,
    pub reputation: Reputation,
    pub traffic: Traffic,
    pub memory: Memory,
    pub rebuild_budget: RebuildBudget,
    pub config: NodeConfig,
    /// The operator's deny list: libp2p peer IDs, IP groups and hex key hashes.
    pub deny: HashSet<String>,
    /// Signs the shared deny list, with `network.share_deny_list` on.
    pub deny_signer: Option<dyapp_identity::Identity>,
    /// The deny list as it is shared, signed when it was last loaded.
    pub shared_deny: Option<SignedRecord>,
    /// Guards (in `GUARDS` order) that refused the last request they checked. A change is
    /// logged once, not per request.
    tripped: [AtomicBool; 5],
    /// Requests each guard refused since the last `report`; the last one counts the byte rate.
    refused: [AtomicU64; 6],
}

const GUARDS: [&str; 6] = [
    "disk: profiles",
    "disk: messages",
    "disk: media",
    "memory: rate limited",
    "memory: dropped",
    "traffic: bytes per second",
];

impl Service {
    pub fn new(store: BootstrapStore, config: NodeConfig) -> Self {
        let l = &config.limits;
        let ban = Duration::from_secs(u64::from(l.ban_minutes) * 60);
        Self {
            store,
            media: None,
            search: None,
            search_peers: Mutex::default(),
            pow: Arc::default(),
            search_sync: Traffic::new(u64::from(config.maintenance.io_mb_per_s) << 20),
            rate_limiter: PeerRateLimiter::new(l.requests_per_second),
            media_peers: PeerRateLimiter::new(l.media_requests_per_second),
            media_bytes: PeerRateLimiter::new(
                u32::try_from(l.bytes_per_second).unwrap_or(u32::MAX).max(1),
            ),
            groups: PeerRateLimiter::new(l.ip_group_requests_per_second),
            senders: PeerRateLimiter::new(l.sender_puts_per_second),
            reputation: Reputation::new(l.strikes_to_ban, ban),
            traffic: Traffic::new(l.bytes_per_second),
            memory: Memory::new(l.max_memory_mb.saturating_mul(1 << 20)),
            rebuild_budget: RebuildBudget::new(config.maintenance.io_mb_per_s),
            config,
            deny: HashSet::new(),
            deny_signer: None,
            shared_deny: None,
            tripped: Default::default(),
            refused: Default::default(),
        }
    }

    /// Logs when a guard starts or stops refusing; returns `tripped`.
    fn guard(&self, index: usize, tripped: bool) -> bool {
        let name = GUARDS[index];
        if tripped {
            self.refused[index].fetch_add(1, Ordering::Relaxed);
        }
        if self.tripped[index].swap(tripped, Ordering::Relaxed) != tripped {
            if tripped {
                tracing::warn!(guard = name, "limit reached, refusing requests");
            } else {
                tracing::info!(guard = name, "limit cleared");
            }
        }
        tripped
    }

    /// Counts `bytes` and whether a request of a role shed at `share` percent of this second's
    /// byte rate is refused. The rate flips every second under load, so it is counted for
    /// `report`, not logged.
    fn shed(&self, bytes: usize, share: u64) -> bool {
        self.traffic.add(bytes as u64);
        self.busy(share)
    }

    pub(crate) fn busy(&self, share: u64) -> bool {
        if self.guard(3, self.memory.share() >= SHED_MEMORY) {
            return true;
        }
        let busy = self.traffic.second() >= share;
        if busy {
            self.refused[5].fetch_add(1, Ordering::Relaxed);
        }
        busy
    }

    /// Whether the process is at `max_memory_mb`: the node then drops requests unanswered, which
    /// resets their streams, and libp2p refuses new connections.
    pub fn overloaded(&self) -> bool {
        self.guard(4, self.memory.share() >= 100)
    }

    /// Logs and resets the refusals per guard; called hourly.
    pub fn report(&self) {
        for (name, refused) in GUARDS.iter().zip(&self.refused) {
            let refused = refused.swap(0, Ordering::Relaxed);
            if refused > 0 {
                tracing::info!(guard = name, refused, "requests refused in the last hour");
            }
        }
    }

    /// Whether a store is at its size limit or its file system at the free-space reserve. A store
    /// at its limit first evicts its oldest records down to 95 % of it; the free-space reserve
    /// is not helped by eviction (freed pages stay in the file until maintenance).
    fn full(
        &self,
        index: usize,
        usage: fn(&BootstrapStore) -> crate::Result<(u64, u64)>,
        evict: fn(&BootstrapStore, u64) -> crate::Result<usize>,
        max_mb: u64,
    ) -> crate::Result<bool> {
        let max = max_mb.saturating_mul(1 << 20);
        let (mut used, free) = usage(&self.store)?;
        if used >= max {
            let evicted = evict(&self.store, max - max / 20)?;
            if evicted > 0 {
                tracing::info!(guard = GUARDS[index], evicted, "evicted the oldest records");
                used = usage(&self.store)?.0;
            }
        }
        let reserve = self.config.limits.min_free_mb.saturating_mul(1 << 20);
        Ok(self.guard(index, used >= max || free <= reserve))
    }

    /// Whether `peer` is within its own and its IP group's request rate; a refusal is a strike.
    fn admit(&self, peer: &Peer) -> bool {
        self.admit_units(peer, 1)
    }

    fn admit_units(&self, peer: &Peer, units: u32) -> bool {
        let admitted = self.admit_quiet(peer, units);
        if !admitted {
            self.strike(peer);
        }
        admitted
    }

    /// Replication can exceed shared buckets through no fault of the sending node.
    fn admit_quiet(&self, peer: &Peer, units: u32) -> bool {
        !self.reputation.banned(&peer.id)
            && self.rate_limiter.check_units(&peer.id, units)
            && self.groups.check_units(&peer.group, units)
    }

    fn node_peer(&self, peer: &Peer) -> bool {
        peer.id
            .parse()
            .is_ok_and(|id| dyapp_p2p_net::id_has_pow(&id, self.config.network.id_pow_bits))
    }

    /// Whether the deny list names `peer` or its IP group. A refusal is no strike: a listed
    /// user is the operator's choice, not misbehaviour.
    fn refused(&self, peer: &Peer) -> bool {
        self.deny.contains(&peer.id) || self.deny.contains(&peer.group)
    }

    /// Whether the deny list names `address` (an identity's peer ID, a mailbox address or a
    /// media blob hash).
    fn listed(&self, address: &[u8]) -> bool {
        self.deny.contains(&hex(address))
    }

    /// Whether the deny list names the hash of `key`.
    fn listed_key(&self, key: &[u8]) -> bool {
        <[u8; 32]>::try_from(key).is_ok_and(|key| self.listed(&dyapp_identity::key_hash(&key)))
    }

    /// Counts misbehaviour of `peer`; the node drops a banned peer's connections.
    pub(crate) fn strike(&self, peer: &Peer) {
        if self.reputation.strike(&peer.id) {
            tracing::warn!(peer = peer.id, group = peer.group, "peer banned");
        }
    }

    pub fn node(&self, peer: &Peer, request: NodeRequest) -> NodeResponse {
        self.traffic.add(request.encoded_len() as u64);
        match request.request {
            Some(node_request::Request::Turn(_)) => self.turn(peer),
            Some(node_request::Request::Info(_)) => NodeResponse {
                status: Status::Ok.into(),
                info: Some(self.info()),
                ..NodeResponse::default()
            },
            Some(node_request::Request::DenyList(_)) => NodeResponse {
                status: match self.shared_deny {
                    Some(_) => Status::Ok,
                    None => Status::NotFound,
                }
                .into(),
                deny_list: self.shared_deny.clone(),
                ..NodeResponse::default()
            },
            None => node_status(Status::Unsupported),
        }
    }

    /// Credentials on the coturn relay next to this node, bound to the asking peer's ID.
    fn turn(&self, peer: &Peer) -> NodeResponse {
        if !self.config.roles.contains(&Role::Turn) {
            return node_status(Status::Unsupported);
        }
        if !self.admit(peer) {
            return node_status(Status::RateLimited);
        }
        if self.refused(peer) {
            return node_status(Status::Refused);
        }
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        NodeResponse {
            turn: Some(turn_credentials(&self.config.turn, &peer.id, now)),
            ..node_status(Status::Ok)
        }
    }

    fn info(&self) -> NodeInfo {
        let store = self.config.roles.contains(&Role::Store);
        let media = self.media.is_some();
        let turn = self.config.roles.contains(&Role::Turn);
        let search = self.search.is_some();
        let roles = [
            (store, proto::Role::Store),
            (media, proto::Role::Media),
            (search, proto::Role::Search),
            (turn, proto::Role::Turn),
        ];
        NodeInfo {
            roles: roles
                .into_iter()
                .filter_map(|(on, role)| on.then_some(role.into()))
                .collect(),
            max_media_bytes: if media { MAX_MEDIA_BYTES as u64 } else { 0 },
            max_profile_bytes: if store { PROFILE_SEND_BYTES as u64 } else { 0 },
            max_message_bytes: if store { MAX_ENVELOPE_BYTES as u64 } else { 0 },
            max_mailbox_bytes: if store { MAX_MAILBOX_BYTES } else { 0 },
            retention_seconds: if store { self.retention() } else { 0 },
            search_fields: if search {
                SEARCH_FIELDS.to_vec()
            } else {
                vec![]
            },
            max_search_profile_bytes: if search {
                self.config.search.max_profile_bytes as u64
            } else {
                0
            },
            min_profile_pow: if search {
                self.config.search.min_profile_pow_bits
            } else {
                0
            },
            server_time: chrono::Utc::now().timestamp().unsigned_abs(),
        }
    }

    fn retention(&self) -> u64 {
        u64::from(self.config.limits.message_ttl_hours) * 3600
    }

    /// `nonce` is the challenge issued on this libp2p connection: `challenge` sets it and the
    /// next `fetch` or `ack` takes it, valid or not, so a captured request cannot be replayed on
    /// this or another connection. Errors as in [`Service::profile`].
    pub fn mailbox(
        &self,
        peer: &Peer,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
    ) -> crate::Result<MailboxResponse> {
        self.mailbox_inner(peer, nonce, request, true)
    }

    /// Validates and bills a client put before the swarm looks up its holders; stores nothing.
    pub fn prepare_mailbox(
        &self,
        peer: &Peer,
        record: SignedRecord,
    ) -> crate::Result<MailboxResponse> {
        self.mailbox_inner(
            peer,
            &mut None,
            MailboxRequest {
                request: Some(mailbox_request::Request::Put(record)),
            },
            false,
        )
    }

    fn mailbox_inner(
        &self,
        peer: &Peer,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
        store: bool,
    ) -> crate::Result<MailboxResponse> {
        if !self.config.roles.contains(&Role::Store) {
            return Ok(status(Status::Unsupported));
        }
        let is_replica = matches!(
            request.request,
            Some(mailbox_request::Request::ReplicaPut(_))
        );
        if is_replica && !self.node_peer(peer) {
            self.strike(peer);
            return Ok(status(Status::Denied));
        }
        let units = if matches!(request.request, Some(mailbox_request::Request::Put(_))) {
            5
        } else {
            1
        };
        if self.shed(request.encoded_len(), 100)
            || !if is_replica {
                self.admit_quiet(peer, units)
            } else {
                self.admit_units(peer, units)
            }
        {
            return Ok(status(Status::RateLimited));
        }
        if self.refused(peer) {
            return Ok(status(Status::Refused));
        }
        let response = self.mailbox_request(nonce, request, store)?;
        if response.status == i32::from(Status::Denied) {
            self.strike(peer);
        }
        Ok(response)
    }

    fn mailbox_request(
        &self,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
        store: bool,
    ) -> crate::Result<MailboxResponse> {
        match request.request {
            Some(mailbox_request::Request::Challenge(_)) => {
                let mut fresh = [0; 32];
                getrandom::fill(&mut fresh)
                    .map_err(|e| BootstrapError::ServerError(e.to_string()))?;
                *nonce = Some(fresh);
                Ok(MailboxResponse {
                    nonce: fresh.to_vec(),
                    ..status(Status::Ok)
                })
            }
            Some(mailbox_request::Request::Put(record)) => {
                self.put(&record, store, true, MAX_ENVELOPE_BYTES)
            }
            Some(mailbox_request::Request::Fetch(record)) => {
                let expected = nonce.take();
                let Ok((mailbox, fetch)) =
                    owner_request::<proto::Fetch>(&record, Domain::MailboxFetch, |f| {
                        fresh(expected, &f.nonce)
                    })
                else {
                    return Ok(status(Status::Denied));
                };
                if self.listed(&mailbox) {
                    return Ok(status(Status::Refused));
                }
                let limit = match fetch.limit {
                    0 => MAX_FETCH,
                    limit => limit.min(MAX_FETCH),
                };
                // `watch` is kept by the node loop, which owns the connections.
                let (envelopes, more) = self.store.fetch_envelopes(&mailbox, limit, FETCH_BYTES)?;
                Ok(MailboxResponse {
                    envelopes,
                    more,
                    ..status(Status::Ok)
                })
            }
            Some(mailbox_request::Request::Ack(record)) => {
                let expected = nonce.take();
                self.ack(&record, |ack| fresh(expected, &ack.nonce))
            }
            // ponytail: a replayed ack deletes only ids its owner already acked; ids are random,
            // so no new envelope reuses them.
            Some(mailbox_request::Request::ReplicaAck(record)) => self.ack(&record, |_| true),
            Some(mailbox_request::Request::Inventory(inventory)) => self.inventory(&inventory),
            Some(mailbox_request::Request::ReplicaPut(batch)) => self.replica_put(&batch),
            None => Ok(status(Status::Unsupported)),
        }
    }

    fn ack(
        &self,
        record: &SignedRecord,
        check: impl Fn(&proto::Ack) -> bool,
    ) -> crate::Result<MailboxResponse> {
        let Ok((mailbox, ack)) = owner_request(record, Domain::MailboxAck, check) else {
            return Ok(status(Status::Denied));
        };
        if ack.ids.iter().any(|id| id.len() != 16) {
            return Ok(status(Status::Invalid));
        }
        self.store.ack_envelopes(&mailbox, &ack.ids)?;
        Ok(status(Status::Ok))
    }

    /// The ids of `inventory` this node lacks; refused from the repair share of the byte rate.
    fn inventory(&self, inventory: &proto::Inventory) -> crate::Result<MailboxResponse> {
        if inventory.mailbox.len() != 32
            || inventory.ids.len() > MAX_INVENTORY
            || inventory.ids.iter().any(|id| id.len() != 16)
        {
            return Ok(status(Status::Invalid));
        }
        if self.listed(&inventory.mailbox) {
            return Ok(status(Status::Refused));
        }
        if self.busy(SHED_MEDIA) {
            return Ok(status(Status::RateLimited));
        }
        let held: HashSet<Vec<u8>> = self.held(&inventory.mailbox)?.into_keys().collect();
        Ok(MailboxResponse {
            missing: inventory
                .ids
                .iter()
                .filter(|id| !held.contains(*id))
                .cloned()
                .collect(),
            ..status(Status::Ok)
        })
    }

    /// One status for the batch: DENIED if any envelope is forged (a strike), else the first
    /// failure. ponytail: the sender rate limit applies per envelope, so a large gap of one
    /// sender fills over several repairs.
    fn replica_put(&self, batch: &proto::Envelopes) -> crate::Result<MailboxResponse> {
        let mut worst = Status::Ok;
        // ponytail: a repaired envelope gets a fresh TTL here, so replicas can re-seed an envelope a
        // watching device never acks until the mailbox cap; carry the expiry if that matters.
        for record in &batch.envelopes {
            let put = Status::try_from(self.put(record, true, true, ENVELOPE_ACCEPT_BYTES)?.status)
                .unwrap_or_default();
            if worst == Status::Ok || put == Status::Denied {
                worst = put;
            }
        }
        Ok(if worst == Status::TooLarge {
            mailbox_too_large(ENVELOPE_ACCEPT_BYTES)
        } else {
            status(worst)
        })
    }

    /// The oldest [`MAX_INVENTORY`] envelopes of a mailbox by id, for repair.
    pub fn held(&self, mailbox: &[u8]) -> crate::Result<HashMap<Vec<u8>, SignedRecord>> {
        let (records, _) = self
            .store
            .fetch_envelopes(mailbox, MAX_INVENTORY as u32, u64::MAX)?;
        Ok(records
            .into_iter()
            .filter_map(|r| Some((proto::Envelope::decode(r.payload.as_slice()).ok()?.id, r)))
            .collect())
    }

    /// Stores a locally held replica after admission, without billing the client twice.
    pub fn store_mailbox(&self, record: &SignedRecord) -> crate::Result<MailboxResponse> {
        self.put(record, true, false, ENVELOPE_ACCEPT_BYTES)
    }

    fn put(
        &self,
        record: &SignedRecord,
        store: bool,
        bill_sender: bool,
        max: usize,
    ) -> crate::Result<MailboxResponse> {
        if record.payload.len() > max {
            return Ok(mailbox_too_large(max));
        }
        if record.verify(Domain::Envelope).is_err() {
            return Ok(status(Status::Denied));
        }
        let Ok(envelope) = proto::Envelope::decode(record.payload.as_slice()) else {
            return Ok(status(Status::Invalid));
        };
        if envelope.id.len() != 16 || envelope.mailbox.len() != 32 {
            return Ok(status(Status::Invalid));
        }
        if self.listed_key(&record.public_key) || self.listed(&envelope.mailbox) {
            return Ok(status(Status::Refused));
        }
        if bill_sender && !self.senders.check_limit(&hex(&record.public_key)) {
            return Ok(status(Status::RateLimited));
        }
        if !store {
            return Ok(status(Status::Ok));
        }
        let max_mb = self.config.limits.messages_max_mb;
        if self.full(
            1,
            BootstrapStore::messages_usage,
            BootstrapStore::evict_envelopes,
            max_mb,
        )? {
            return Ok(status(Status::Full));
        }
        let expires_at = chrono::Utc::now().timestamp() + self.retention() as i64;
        let stored = self.store.put_envelope(
            &envelope.mailbox,
            &envelope.id,
            record,
            expires_at,
            MAX_MAILBOX_BYTES,
        )?;
        Ok(status(if stored { Status::Ok } else { Status::Full }))
    }

    /// `peer` is the key for rate limiting and strikes. A storage failure is an
    /// error: the caller drops the request and the client tries another node.
    pub fn profile(&self, peer: &Peer, request: ProfileRequest) -> crate::Result<ProfileResponse> {
        self.profile_inner(peer, request, true)
    }

    pub fn prepare_profile(
        &self,
        peer: &Peer,
        record: SignedRecord,
    ) -> crate::Result<ProfileResponse> {
        self.profile_inner(
            peer,
            ProfileRequest {
                request: Some(profile_request::Request::Publish(record)),
            },
            false,
        )
    }

    fn profile_inner(
        &self,
        peer: &Peer,
        request: ProfileRequest,
        store: bool,
    ) -> crate::Result<ProfileResponse> {
        if !self.config.roles.contains(&Role::Store) {
            return Ok(reply(Status::Unsupported, None));
        }
        let is_replica = matches!(
            request.request,
            Some(profile_request::Request::ReplicaPut(_))
        );
        if is_replica && !self.node_peer(peer) {
            self.strike(peer);
            return Ok(reply(Status::Denied, None));
        }
        let units = if matches!(request.request, Some(profile_request::Request::Publish(_))) {
            5
        } else {
            1
        };
        let share = if matches!(
            request.request,
            Some(profile_request::Request::Inventory(_))
        ) {
            SHED_MEDIA
        } else {
            SHED_PROFILES
        };
        if self.shed(request.encoded_len(), share)
            || !if is_replica {
                self.admit_quiet(peer, units)
            } else {
                self.admit_units(peer, units)
            }
        {
            return Ok(reply(Status::RateLimited, None));
        }
        if self.refused(peer) {
            return Ok(reply(Status::Refused, None));
        }
        match request.request {
            Some(
                profile_request::Request::Publish(record)
                | profile_request::Request::ReplicaPut(record),
            ) => {
                let max = if is_replica {
                    PROFILE_ACCEPT_BYTES
                } else {
                    PROFILE_SEND_BYTES
                };
                let response = self.publish(&record, store, max)?;
                if response.status == i32::from(Status::Denied) {
                    self.strike(peer);
                }
                Ok(response)
            }
            Some(profile_request::Request::Get(get)) => {
                let Ok(peer_id) = <[u8; 32]>::try_from(get.peer_id.as_slice()) else {
                    return Ok(reply(Status::Invalid, None));
                };
                if self.listed(&peer_id) {
                    return Ok(reply(Status::Refused, None));
                }
                Ok(match self.store.get_profile(&hex(&peer_id))? {
                    Some(record) => reply(Status::Ok, Some(record)),
                    None => reply(Status::NotFound, None),
                })
            }
            Some(profile_request::Request::Inventory(get)) => {
                let Ok(peer_id) = <[u8; 32]>::try_from(get.peer_id.as_slice()) else {
                    return Ok(reply(Status::Invalid, None));
                };
                if self.listed(&peer_id) {
                    return Ok(reply(Status::Refused, None));
                }
                let response = match self.store.get_profile(&hex(&peer_id))? {
                    Some(record) => ProfileResponse {
                        version: dyapp_profile::stored_version(&record)?,
                        ..reply(Status::Ok, None)
                    },
                    None => reply(Status::NotFound, None),
                };
                Ok(response)
            }
            Some(profile_request::Request::Heartbeat(record)) => {
                let Ok((owner, beat)) =
                    owner_request::<proto::Heartbeat>(&record, Domain::Heartbeat, |_| true)
                else {
                    self.strike(peer);
                    return Ok(reply(Status::Denied, None));
                };
                if self.listed(&owner) {
                    return Ok(reply(Status::Refused, None));
                }
                let now = chrono::Utc::now().timestamp();
                if beat.time.abs_diff(now.unsigned_abs()) > CLOCK_SKEW {
                    return Ok(ProfileResponse {
                        server_time: now.unsigned_abs(),
                        ..reply(Status::Invalid, None)
                    });
                }
                let Ok(time) = i64::try_from(beat.time) else {
                    return Ok(ProfileResponse {
                        server_time: now.unsigned_abs(),
                        ..reply(Status::Invalid, None)
                    });
                };
                let found = self.store.touch_profile(&hex(&owner), time)?;
                Ok(reply(
                    if found { Status::Ok } else { Status::NotFound },
                    None,
                ))
            }
            None => Ok(reply(Status::Unsupported, None)),
        }
    }

    pub fn store_profile(&self, record: &SignedRecord) -> crate::Result<ProfileResponse> {
        self.publish(record, true, PROFILE_ACCEPT_BYTES)
    }

    fn publish(
        &self,
        record: &SignedRecord,
        store: bool,
        max: usize,
    ) -> crate::Result<ProfileResponse> {
        if record.payload.len() > max {
            return Ok(profile_too_large(max));
        }
        if self.listed_key(&record.public_key) {
            return Ok(reply(Status::Refused, None));
        }
        match dyapp_profile::verify(record) {
            Ok(_) => {}
            Err(dyapp_profile::Error::Signature(_)) => return Ok(reply(Status::Denied, None)),
            Err(_) => return Ok(reply(Status::Invalid, None)),
        }
        if !store {
            return Ok(reply(Status::Ok, None));
        }
        let max_mb = self.config.limits.profiles_max_mb;
        if self.full(
            0,
            BootstrapStore::profiles_usage,
            BootstrapStore::evict_profiles,
            max_mb,
        )? {
            return Ok(reply(Status::Full, None));
        }
        match self.store.put_profile(record) {
            Ok(_) => Ok(reply(Status::Ok, None)),
            Err(BootstrapError::Profile(dyapp_profile::Error::Stale)) => {
                // A stale record verified, so its key is a valid 32-byte key.
                let key: [u8; 32] = record.public_key.as_slice().try_into().unwrap_or_default();
                let stored = self.store.get_profile(&dyapp_identity::peer_id(&key))?;
                Ok(reply(Status::Stale, stored))
            }
            Err(BootstrapError::Profile(dyapp_profile::Error::Signature(_))) => {
                Ok(reply(Status::Denied, None))
            }
            Err(BootstrapError::Profile(_)) => Ok(reply(Status::Invalid, None)),
            Err(error) => Err(error),
        }
    }
}

/// What the node does with a search gossip message.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    Ignore,
    Reject,
    /// A proof of work not cached yet: check it off the loop, then apply the message again.
    Pow(PowCheck),
}

/// What catch-up did with one record.
#[derive(Debug, PartialEq, Eq)]
pub enum Caught {
    Stored,
    Dropped,
    /// A proof of work not cached yet: check it off the loop, then apply the record again.
    Pow(PowCheck),
}

/// Search get replies are cut here, like the profile record limit.
const MAX_SEARCH_REPLY: usize = 1 << 20;
/// Keys in one search get.
const MAX_SEARCH_GET: usize = 50;
/// Keys in one inventory reply.
const MAX_SEARCH_INVENTORY: u32 = 10_000;

fn search_reply(status: Status) -> SearchResponse {
    SearchResponse {
        status: status.into(),
        ..SearchResponse::default()
    }
}

/// The key hash of a publish or heartbeat, which picks its gossip topic.
pub fn search_key(request: &SearchRequest) -> Option<[u8; 32]> {
    match &request.request {
        Some(search_request::Request::Publish(r) | search_request::Request::Heartbeat(r)) => {
            let key: [u8; 32] = r.public_key.as_slice().try_into().ok()?;
            Some(dyapp_identity::key_hash(&key))
        }
        _ => None,
    }
}

/// What applying a publish or heartbeat did, before it becomes a reply or a gossip verdict.
enum Applied {
    Done,
    /// Valid, but of a topic this node does not keep.
    Skipped,
    /// A bad signature: a strike for the sender.
    Bad,
    Status(Status),
    TooLarge(usize),
    RequiredPow(u32),
    Clock(u64),
    Limited(u64),
    /// The proof of work is not cached: it needs an Argon2 check first.
    Pow(PowCheck),
}

/// A search request after admission: a ready reply, or read work the node runs in
/// `spawn_blocking` so a slow query never holds the swarm loop.
pub enum Search {
    Reply(SearchResponse),
    Query(proto::SearchQuery),
    Get(Vec<[u8; 32]>),
    /// A publish whose proof of work is not cached: the node checks it off the loop, then
    /// calls [`Service::search_pow_done`].
    Pow(PowCheck, SignedRecord),
}

pub fn run_query(
    index: &SearchIndex,
    query: &proto::SearchQuery,
    max_scan_rows: u32,
) -> crate::Result<SearchResponse> {
    Ok(
        match index.query(&query.conditions, query.limit, max_scan_rows)? {
            Err(()) => search_reply(Status::Invalid),
            Ok(found) => SearchResponse {
                hits: found
                    .hits
                    .into_iter()
                    .map(|(key, version)| proto::SearchHit {
                        key: key.to_vec(),
                        version,
                    })
                    .collect(),
                applied: found.applied,
                partial: found.partial,
                index_version: crate::search::INDEX_VERSION,
                ..search_reply(Status::Ok)
            },
        },
    )
}

pub fn run_get(index: &SearchIndex, keys: &[[u8; 32]]) -> crate::Result<SearchResponse> {
    Ok(SearchResponse {
        records: index.records(keys, MAX_SEARCH_REPLY)?,
        ..search_reply(Status::Ok)
    })
}

impl Service {
    /// Errors as in [`Service::profile`].
    pub fn search(&self, peer: &Peer, request: SearchRequest) -> crate::Result<Search> {
        let reply = |status| Ok(Search::Reply(search_reply(status)));
        if self.search.is_none() {
            return reply(Status::Unsupported);
        }
        // A get is also charged by its reply size in `charge_get`; search nodes only there.
        let units = match &request.request {
            Some(search_request::Request::Publish(_) | search_request::Request::Heartbeat(_)) => 5,
            Some(search_request::Request::Get(_)) if self.is_search_peer(peer) => 0,
            _ => 1,
        };
        if self.shed(request.encoded_len(), SHED_MEDIA)
            || (units > 0 && !self.admit_units(peer, units))
        {
            return reply(Status::RateLimited);
        }
        if self.refused(peer) {
            return reply(Status::Refused);
        }
        Ok(match request.request {
            Some(search_request::Request::Publish(record)) => {
                match self.publish_search(&record, true, self.config.search.max_profile_bytes)? {
                    Applied::Pow(check) => Search::Pow(check, record),
                    applied => Search::Reply(self.applied(peer, applied)),
                }
            }
            Some(search_request::Request::Heartbeat(record)) => {
                Search::Reply(self.applied(peer, self.beat_search(&record)?))
            }
            Some(search_request::Request::Query(query)) => Search::Query(query),
            Some(search_request::Request::Get(get)) => {
                let keys: Option<Vec<[u8; 32]>> = (get.keys.len() <= MAX_SEARCH_GET)
                    .then(|| {
                        get.keys
                            .iter()
                            .map(|k| k.as_slice().try_into().ok())
                            .collect()
                    })
                    .flatten();
                let Some(keys) = keys else {
                    return reply(Status::Invalid);
                };
                Search::Get(keys.into_iter().filter(|k| !self.listed(k)).collect())
            }
            None => Search::Reply(search_reply(Status::Invalid)),
        })
    }

    /// Charges a built get reply: a unit per started 64 KiB for clients, `search_sync` bytes for
    /// search nodes. `false` means the node replies `RATE_LIMITED` instead.
    pub fn charge_get(&self, peer: &Peer, bytes: usize) -> bool {
        if self.is_search_peer(peer) {
            return self.search_sync.try_add(bytes as u64, 100);
        }
        let units = u32::try_from(bytes.div_ceil(64 << 10)).unwrap_or(u32::MAX);
        units == 0 || self.admit_units(peer, units)
    }

    /// Marks `peer_id` as a connected search node or forgets it.
    pub fn search_peer(&self, peer_id: &str, on: bool) {
        let Ok(mut peers) = self.search_peers.lock() else {
            return;
        };
        if on {
            peers.insert(peer_id.to_owned());
        } else {
            peers.remove(peer_id);
        }
    }

    fn is_search_peer(&self, peer: &Peer) -> bool {
        self.search_peers
            .lock()
            .is_ok_and(|peers| peers.contains(&peer.id))
    }

    fn applied(&self, peer: &Peer, applied: Applied) -> SearchResponse {
        match applied {
            Applied::Done | Applied::Skipped => search_reply(Status::Ok),
            Applied::Bad => {
                self.strike(peer);
                search_reply(Status::Denied)
            }
            Applied::Status(status) => search_reply(status),
            Applied::TooLarge(max) => SearchResponse {
                max_bytes: max as u64,
                ..search_reply(Status::TooLarge)
            },
            Applied::RequiredPow(required_pow_bits) => SearchResponse {
                required_pow_bits,
                ..search_reply(Status::Invalid)
            },
            Applied::Clock(server_time) => SearchResponse {
                server_time,
                ..search_reply(Status::Invalid)
            },
            Applied::Limited(retry_after) => SearchResponse {
                retry_after,
                ..search_reply(Status::RateLimited)
            },
            // The cache was cleared between the check and this run: the client retries.
            Applied::Pow(_) => SearchResponse {
                retry_after: 1,
                ..search_reply(Status::RateLimited)
            },
        }
    }

    /// Finishes a publish after its off-loop proof-of-work check; a forged proof is a strike.
    pub fn search_pow_done(
        &self,
        peer: &Peer,
        record: &SignedRecord,
        ok: bool,
    ) -> crate::Result<SearchResponse> {
        if !ok {
            return Ok(self.applied(peer, Applied::Bad));
        }
        Ok(self.applied(
            peer,
            self.publish_search(record, true, self.config.search.max_profile_bytes)?,
        ))
    }

    /// `limit = false` (catch-up) skips only the 10-minute update limit.
    fn publish_search(
        &self,
        record: &SignedRecord,
        limit: bool,
        max: usize,
    ) -> crate::Result<Applied> {
        let Some(index) = &self.search else {
            return Ok(Applied::Status(Status::Unsupported));
        };
        if record.encoded_len() > max {
            return Ok(Applied::TooLarge(max));
        }
        let verified = match dyapp_profile::verify(record) {
            Ok(verified) => verified,
            Err(dyapp_profile::Error::Signature(_)) => return Ok(Applied::Bad),
            Err(_) => return Ok(Applied::Status(Status::Invalid)),
        };
        // A verified record has a valid 32-byte key.
        let signer: [u8; 32] = record.public_key.as_slice().try_into().unwrap_or_default();
        let key = dyapp_identity::key_hash(&signer);
        if self.listed(&key) {
            return Ok(Applied::Status(Status::Refused));
        }
        if !self.config.search.keeps(crate::search::topic(&key)) {
            return Ok(Applied::Skipped);
        }
        let profile = &verified.profile;
        if profile.pow_bits < self.config.search.min_profile_pow_bits {
            return Ok(Applied::RequiredPow(
                self.config.search.min_profile_pow_bits,
            ));
        }
        if !self
            .pow
            .confirmed(&signer, profile.pow_nonce, profile.pow_bits)
        {
            return Ok(Applied::Pow(PowCheck {
                key: signer,
                nonce: profile.pow_nonce,
                bits: profile.pow_bits,
            }));
        }
        let now = chrono::Utc::now().timestamp();
        Ok(
            match index.put(&key, &verified.profile, record, now, limit)? {
                crate::search::Put::Stored => Applied::Done,
                crate::search::Put::Stale => Applied::Status(Status::Stale),
                crate::search::Put::Limited { retry_after } => Applied::Limited(retry_after),
            },
        )
    }

    fn beat_search(&self, record: &SignedRecord) -> crate::Result<Applied> {
        let Some(index) = &self.search else {
            return Ok(Applied::Status(Status::Unsupported));
        };
        let Ok((key, beat)) =
            owner_request::<proto::Heartbeat>(record, Domain::Heartbeat, |_| true)
        else {
            return Ok(Applied::Bad);
        };
        if self.listed(&key) {
            return Ok(Applied::Status(Status::Refused));
        }
        let now = chrono::Utc::now().timestamp();
        if beat.time.abs_diff(now.unsigned_abs()) > CLOCK_SKEW {
            return Ok(Applied::Clock(now.unsigned_abs()));
        }
        if !self.config.search.keeps(crate::search::topic(&key)) {
            return Ok(Applied::Skipped);
        }
        let min = self.config.search.min_profile_pow_bits;
        if min > 0 && index.pow_bits(&key)?.is_some_and(|bits| bits < min) {
            return Ok(Applied::RequiredPow(min));
        }
        Ok(match index.heartbeat(&key, beat.version, now)? {
            crate::search::Beat::Ok => Applied::Done,
            crate::search::Beat::NotFound => Applied::Status(Status::NotFound),
            crate::search::Beat::Limited { retry_after } => Applied::Limited(retry_after),
        })
    }

    /// Applies a gossiped publish or heartbeat. On `Reject` the node strikes the peer that
    /// propagated it.
    pub fn search_gossip(&self, data: &[u8]) -> Verdict {
        let Ok(request) = SearchRequest::decode(data) else {
            return Verdict::Reject;
        };
        let applied = match &request.request {
            Some(search_request::Request::Publish(record)) => {
                self.publish_search(record, true, SEARCH_ACCEPT_BYTES)
            }
            Some(search_request::Request::Heartbeat(record)) => self.beat_search(record),
            _ => return Verdict::Reject,
        };
        match applied {
            Ok(Applied::Done) => Verdict::Accept,
            Ok(Applied::Bad) => Verdict::Reject,
            Ok(Applied::Pow(check)) => Verdict::Pow(check),
            Ok(_) => Verdict::Ignore,
            Err(error) => {
                tracing::error!(%error, "search gossip not stored");
                Verdict::Ignore
            }
        }
    }

    /// Stores a record fetched in catch-up, without the update limit.
    pub fn search_apply(&self, record: &SignedRecord) -> crate::Result<Caught> {
        Ok(
            match self.publish_search(record, false, SEARCH_ACCEPT_BYTES)? {
                Applied::Done => Caught::Stored,
                Applied::Pow(check) => Caught::Pow(check),
                _ => Caught::Dropped,
            },
        )
    }

    /// `/dyapp/inventory`: the key hashes and versions of one topic, for catch-up.
    pub fn search_inventory(
        &self,
        peer: &Peer,
        request: InventoryRequest,
    ) -> crate::Result<InventoryResponse> {
        let reply = |status: Status| InventoryResponse {
            status: status.into(),
            have: vec![],
        };
        let Some(index) = &self.search else {
            return Ok(reply(Status::Unsupported));
        };
        let sync = self.is_search_peer(peer);
        if self.shed(request.encoded_len(), SHED_MEDIA) || (!sync && !self.admit_units(peer, 10)) {
            return Ok(reply(Status::RateLimited));
        }
        if self.refused(peer) {
            return Ok(reply(Status::Refused));
        }
        if request.topic >= crate::search::SEARCH_TOPICS || request.after.len() > 32 {
            return Ok(reply(Status::Invalid));
        }
        let have = index.inventory(request.topic, &request.after, &[], MAX_SEARCH_INVENTORY)?;
        let response = InventoryResponse {
            status: Status::Ok.into(),
            have: have
                .into_iter()
                .map(|(key, version)| proto::SearchHit {
                    key: key.to_vec(),
                    version,
                })
                .collect(),
        };
        if sync && !self.search_sync.try_add(response.encoded_len() as u64, 100) {
            return Ok(reply(Status::RateLimited));
        }
        Ok(response)
    }
}

impl Service {
    /// Errors as in [`Service::profile`].
    pub fn media(&self, peer: &Peer, request: MediaRequest) -> crate::Result<MediaResponse> {
        if let Some(media_request::Request::Put(put)) = &request.request {
            return match self.prepare_media(peer, put)? {
                Ok(replica) => self.store_media(&replica),
                Err(status) => Ok(media_upload_status(status)),
            };
        }
        let Some(media) = &self.media else {
            return Ok(media_status(Status::Unsupported));
        };
        let is_replica = matches!(
            request.request,
            Some(media_request::Request::ReplicaPut(_) | media_request::Request::ShardPut(_))
        );
        if is_replica && !self.node_peer(peer) {
            self.strike(peer);
            return Ok(media_status(Status::Denied));
        }
        // The media limit is no strike: a client loading a gallery is not misbehaving.
        if self.shed(request.encoded_len(), SHED_MEDIA)
            || !if is_replica {
                self.admit_quiet(peer, 1)
            } else {
                self.admit(peer)
            }
            || !self.media_peers.check_limit(&peer.id)
        {
            return Ok(media_status(Status::RateLimited));
        }
        if self.refused(peer) {
            return Ok(media_status(Status::Refused));
        }
        let l = &self.config.limits;
        Ok(match request.request {
            Some(media_request::Request::Keep(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, keep)) =
                    owner_request::<proto::MediaKeep>(&record, Domain::MediaKeep, |_| true)
                else {
                    self.strike(peer);
                    return Ok(media_status(Status::Denied));
                };
                if self.listed(&owner) {
                    return Ok(media_status(Status::Refused));
                }
                let now = chrono::Utc::now().timestamp().unsigned_abs();
                if keep.version == 0
                    || keep.hashes.len() > MAX_KEEP
                    || keep.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if keep.time.abs_diff(now) > CLOCK_SKEW {
                    return Ok(media_clock_invalid(now));
                }
                match media.keep_signed(&hex(&owner), &keep, &record)? {
                    Some(missing) => MediaResponse {
                        missing,
                        ..media_status(Status::Ok)
                    },
                    None => media_status(Status::Stale),
                }
            }
            Some(media_request::Request::Put(_)) => unreachable!("put admitted separately"),
            // ponytail: a get names no owner, so a listed owner's blobs already stored are still
            // served unless the deny list names the blob hash too.
            Some(media_request::Request::Get(get)) => {
                if get.hash.len() != 32 {
                    return Ok(media_status(Status::Invalid));
                }
                if self.listed(&get.hash) {
                    return Ok(media_status(Status::Refused));
                }
                match media.get(&get.hash)? {
                    Some(data) => {
                        if !self.media_charge(peer, data.len() as u64, data.len() as u64) {
                            return Ok(media_status(Status::RateLimited));
                        }
                        MediaResponse {
                            data,
                            ..media_status(Status::Ok)
                        }
                    }
                    None => media_status(Status::NotFound),
                }
            }
            Some(media_request::Request::Attach(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, attach)) =
                    owner_request::<proto::MediaAttach>(&record, Domain::MediaAttach, |_| true)
                else {
                    self.strike(peer);
                    return Ok(media_status(Status::Denied));
                };
                if self.listed(&owner) {
                    return Ok(media_status(Status::Refused));
                }
                let now = chrono::Utc::now().timestamp().unsigned_abs();
                if attach.release.len() != 32
                    || attach.hashes.is_empty()
                    || attach.hashes.len() > MAX_ATTACH
                    || attach.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if attach.created > now.saturating_add(CLOCK_SKEW) {
                    return Ok(media_clock_invalid(now));
                }
                let expires = attach.created + u64::from(l.attachment_retention_hours) * 3600;
                match media.attach_signed(
                    &hex(&owner),
                    &attach,
                    expires,
                    MAX_ATTACHMENTS,
                    &record,
                )? {
                    Some(missing) => MediaResponse {
                        missing,
                        ..media_status(Status::Ok)
                    },
                    None => media_status(Status::Full),
                }
            }
            Some(media_request::Request::Release(release)) => {
                media_status(if media.release(&release.secret)? {
                    Status::Ok
                } else {
                    Status::NotFound
                })
            }
            Some(media_request::Request::ReplicaPut(replica)) => {
                if !self.media_charge(peer, replica.data.len() as u64, 0) {
                    return Ok(media_status(Status::RateLimited));
                }
                let response = self.store_media(&replica)?;
                if response.status == i32::from(Status::Denied) {
                    self.strike(peer);
                }
                response
            }
            Some(media_request::Request::ShardPut(put)) => {
                if !self.media_charge(peer, put.data.len() as u64, 0) {
                    return Ok(media_status(Status::RateLimited));
                }
                let response = self.store_shard(&put)?;
                if response.status == i32::from(Status::Denied) {
                    self.strike(peer);
                }
                response
            }
            Some(media_request::Request::Have(have)) => {
                if have.items.len() > crate::media_repair::MAX_ITEMS
                    || have
                        .items
                        .iter()
                        .map(|p| p.hash.as_slice())
                        .collect::<HashSet<_>>()
                        .len()
                        > MAX_KEEP
                    || have
                        .items
                        .iter()
                        .any(|p| p.hash.len() != 32 || p.index.is_some_and(|i| i >= 10))
                {
                    media_status(Status::Invalid)
                } else {
                    // The swarm applies trust and key proximity before reading inventory.
                    media_status(Status::Ok)
                }
            }
            Some(media_request::Request::ShardGet(get)) => {
                if get.hash.len() != 32 || get.index >= 10 {
                    return Ok(media_status(Status::Invalid));
                }
                if self.listed(&get.hash) {
                    return Ok(media_status(Status::Refused));
                }
                match media.shard(&get.hash, get.index as usize)? {
                    Some(data) if self.media_charge(peer, data.len() as u64, data.len() as u64) => {
                        MediaResponse {
                            data,
                            ..media_status(Status::Ok)
                        }
                    }
                    Some(_) => media_status(Status::RateLimited),
                    None => media_status(Status::NotFound),
                }
            }
            None => media_status(Status::Unsupported),
        })
    }

    pub fn media_inventory(&self, items: &[proto::MediaPart]) -> crate::Result<MediaResponse> {
        let media = self.media.as_ref().unwrap();
        let mut inventory = proto::MediaInventory::default();
        let mut hashes = HashSet::new();
        for part in items {
            if part.hash.len() != 32
                || part.index.is_some_and(|i| i >= 10)
                || self.listed(&part.hash)
            {
                continue;
            }
            if hashes.insert(part.hash.clone()) {
                if media.get(&part.hash)?.is_some_and(|d| {
                    d.len() <= crate::replication::MEDIA_OBJECT_BYTES
                        && dyapp_identity::sha256(&d).as_slice() == part.hash
                }) {
                    inventory.whole.push(part.hash.clone());
                } else if let Some(manifest) = media
                    .manifest(&part.hash)?
                    .filter(|m| m.hash == part.hash && m.data_shards().is_some())
                {
                    inventory.manifests.push(manifest);
                }
            }
            let whole = inventory.whole.contains(&part.hash);
            let manifest = inventory.manifests.iter().find(|m| m.hash == part.hash);
            let present = match part.index {
                None => whole || manifest.is_some(),
                Some(index) => {
                    if whole {
                        media.shard(&part.hash, index as usize)?.is_some()
                    } else if let Some(manifest) = manifest {
                        media
                            .shard(&part.hash, index as usize)?
                            .is_some_and(|d| manifest.accepts(index as usize, &d))
                    } else {
                        false
                    }
                }
            };
            let list = if present {
                &mut inventory.present
            } else {
                &mut inventory.missing
            };
            if !list.contains(part) {
                list.push(part.clone());
            }
        }
        Ok(MediaResponse {
            inventory: Some(inventory),
            ..media_status(Status::Ok)
        })
    }

    pub fn store_shard(&self, put: &proto::MediaShardPut) -> crate::Result<MediaResponse> {
        let Some(replica) = &put.replica else {
            return Ok(media_status(Status::Denied));
        };
        let Some(manifest) = &replica.manifest else {
            return Ok(media_status(Status::Invalid));
        };
        if !manifest.accepts(put.index as usize, &put.data) {
            return Ok(media_status(Status::Invalid));
        }
        let response = self.store_media(replica)?;
        if response.status != i32::from(Status::Ok) {
            return Ok(response);
        }
        Ok(media_status(
            match self
                .media
                .as_ref()
                .unwrap()
                .put_shard(manifest, put.index as usize, &put.data)?
            {
                Put::Stored => Status::Ok,
                Put::Invalid => Status::Invalid,
                Put::NotListed => Status::NotFound,
                Put::OverQuota => Status::Full,
            },
        ))
    }

    pub fn assemble_media(
        &self,
        manifest: &proto::MediaManifest,
        shards: &[(usize, Vec<u8>)],
    ) -> crate::Result<MediaResponse> {
        let media = self.media.as_ref().unwrap();
        if let Some(data) = media.get(&manifest.hash)? {
            if dyapp_identity::sha256(&data).as_slice() == manifest.hash {
                return Ok(MediaResponse {
                    data,
                    ..media_status(Status::Ok)
                });
            }
        }
        match crate::replication::media_decode(manifest, shards) {
            Ok(data) => Ok(MediaResponse {
                data,
                ..media_status(Status::Ok)
            }),
            Err(error) => {
                tracing::warn!(%error, "media decode failed");
                media.discard_sharded(&manifest.hash)?;
                Ok(media_status(Status::NotFound))
            }
        }
    }

    pub fn prepare_media_read(&self, peer: &Peer, manifest: &proto::MediaManifest) -> bool {
        let Some(k) = manifest.data_shards() else {
            return false;
        };
        let assembly =
            manifest.shard_size().unwrap() as u64 * (k + crate::replication::MEDIA_PARITY) as u64;
        self.media_charge(peer, manifest.length, manifest.length + assembly)
    }

    /// Byte charges are separate from the request-count buckets; 0 disables the byte cap.
    fn media_charge(&self, peer: &Peer, bytes: u64, reserve: u64) -> bool {
        let admitted = self.config.limits.bytes_per_second == 0
            || bytes == 0
            || self
                .media_bytes
                .check_units(&peer.id, u32::try_from(bytes).unwrap_or(u32::MAX));
        admitted && self.traffic.try_add(reserve, SHED_MEDIA)
    }

    /// Accepts one upload without storing locally unless this node holds a replica key.
    pub fn prepare_media(
        &self,
        peer: &Peer,
        put: &proto::MediaPut,
    ) -> crate::Result<std::result::Result<proto::MediaReplicaPut, Status>> {
        let Some(media) = &self.media else {
            return Ok(Err(Status::Unsupported));
        };
        let request = MediaRequest {
            request: Some(media_request::Request::Put(put.clone())),
        };
        if self.shed(request.encoded_len(), SHED_MEDIA)
            || !self.admit(peer)
            || !self.media_peers.check_limit(&peer.id)
        {
            return Ok(Err(Status::RateLimited));
        }
        if self.refused(peer)
            || self.listed(&put.owner)
            || self.listed(&dyapp_identity::sha256(&put.data))
        {
            return Ok(Err(Status::Refused));
        }
        if put.data.len() > MAX_MEDIA_BYTES {
            return Ok(Err(Status::TooLarge));
        }
        if put.owner.len() != 32 {
            return Ok(Err(Status::Invalid));
        }
        let hash = dyapp_identity::sha256(&put.data);
        let Some(authorization) = media.authorization(&hex(&put.owner), &hash)? else {
            return Ok(Err(Status::NotFound));
        };
        let bytes = put.data.len() as u64;
        let billed = if put.data.len() > self.config.media.shard_threshold {
            let k = bytes.div_ceil(crate::replication::MEDIA_OBJECT_BYTES as u64);
            (bytes * (k + crate::replication::MEDIA_PARITY as u64)).div_ceil(k)
        } else {
            bytes * u64::from(dyapp_p2p_net::REPLICAS)
        };
        if !self.media_charge(peer, billed, billed.saturating_sub(bytes)) {
            return Ok(Err(Status::RateLimited));
        }
        Ok(Ok(proto::MediaReplicaPut {
            data: put.data.clone(),
            authorization: Some(authorization),
            manifest: None,
        }))
    }

    /// Independently verifies the authorization and checks against the latest local keep.
    pub fn store_media(&self, replica: &proto::MediaReplicaPut) -> crate::Result<MediaResponse> {
        use proto::media_replica_put::Authorization;
        let Some(media) = &self.media else {
            return Ok(media_status(Status::Unsupported));
        };
        if replica.data.len() > crate::replication::MEDIA_OBJECT_BYTES {
            return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
        }
        let hash = if let Some(manifest) = &replica.manifest {
            if !replica.data.is_empty() || manifest.data_shards().is_none() {
                return Ok(media_status(Status::Invalid));
            }
            manifest.hash.clone()
        } else {
            dyapp_identity::sha256(&replica.data).to_vec()
        };
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let l = &self.config.limits;
        let owner = match &replica.authorization {
            Some(Authorization::Keep(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, keep)) =
                    owner_request::<proto::MediaKeep>(record, Domain::MediaKeep, |_| true)
                else {
                    return Ok(media_status(Status::Denied));
                };
                if keep.version == 0
                    || keep.hashes.len() > MAX_KEEP
                    || keep.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if keep.time > now.saturating_add(CLOCK_SKEW)
                    || now.saturating_sub(keep.time) > u64::from(l.profile_ttl_days) * 86400
                {
                    return Ok(media_clock_invalid(now));
                }
                if self.listed(&owner) || self.listed(&hash) {
                    return Ok(media_status(Status::Refused));
                }
                if !keep.hashes.iter().any(|h| h.as_slice() == hash) {
                    return Ok(media_status(Status::NotFound));
                }
                media.keep_signed(&hex(&owner), &keep, record)?;
                if !media.kept(&hex(&owner), &hash)? {
                    return Ok(media_status(Status::NotFound));
                }
                owner
            }
            Some(Authorization::Attach(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, attach)) =
                    owner_request::<proto::MediaAttach>(record, Domain::MediaAttach, |_| true)
                else {
                    return Ok(media_status(Status::Denied));
                };
                let expires = attach
                    .created
                    .saturating_add(u64::from(l.attachment_retention_hours) * 3600);
                if attach.release.len() != 32
                    || attach.hashes.is_empty()
                    || attach.hashes.len() > MAX_ATTACH
                    || attach.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if attach.created > now.saturating_add(CLOCK_SKEW) || expires <= now {
                    return Ok(media_clock_invalid(now));
                }
                if self.listed(&owner) || self.listed(&hash) {
                    return Ok(media_status(Status::Refused));
                }
                if !attach.hashes.iter().any(|h| h.as_slice() == hash) {
                    return Ok(media_status(Status::NotFound));
                }
                if media
                    .attach_signed(&hex(&owner), &attach, expires, MAX_ATTACHMENTS, record)?
                    .is_none()
                {
                    return Ok(media_status(Status::Full));
                }
                owner
            }
            None => return Ok(media_status(Status::Denied)),
        };
        let max = l.media_max_mb.saturating_mul(1 << 20);
        let (mut used, mut free) = media.usage()?;
        if used >= max {
            let removed = media.evict(max / 100 * 95)?;
            tracing::info!(removed, "media lists evicted");
            (used, free) = media.usage()?;
        }
        if self.guard(
            2,
            used >= max || free <= l.min_free_mb.saturating_mul(1 << 20),
        ) {
            return Ok(media_status(Status::Full));
        }
        // Whole-copy ownership is charged at R=5 even when keys co-locate on a small network.
        let quota = l.media_per_owner_mb.saturating_mul(1 << 20);
        let stored = if let Some(manifest) = &replica.manifest {
            media.put_manifest(&hex(&owner), manifest, quota)?
        } else {
            media.put(
                &hex(&owner),
                &replica.data,
                quota / u64::from(dyapp_p2p_net::REPLICAS),
            )?
        };
        Ok(media_status(match stored {
            Put::Stored => Status::Ok,
            Put::NotListed => Status::NotFound,
            Put::OverQuota => Status::Full,
            Put::Invalid => Status::Invalid,
        }))
    }
}

fn media_clock_invalid(server_time: u64) -> MediaResponse {
    MediaResponse {
        server_time,
        ..media_status(Status::Invalid)
    }
}

/// Converts prepare_media's upload rejection without changing its Status interface.
pub(crate) fn media_upload_status(status: Status) -> MediaResponse {
    if status == Status::TooLarge {
        media_too_large(MAX_MEDIA_BYTES)
    } else {
        media_status(status)
    }
}

fn media_too_large(max: usize) -> MediaResponse {
    MediaResponse {
        max_bytes: max as u64,
        ..media_status(Status::TooLarge)
    }
}

fn media_status(status: Status) -> MediaResponse {
    MediaResponse {
        status: status.into(),
        ..MediaResponse::default()
    }
}

/// Checks a request signed by a mailbox's device key or an owner's identity key and passing
/// `check` (its nonce); returns the hash of the signing key (the mailbox address or the peer ID)
/// and the decoded request.
fn owner_request<M: Message + Default>(
    record: &SignedRecord,
    domain: Domain,
    check: impl Fn(&M) -> bool,
) -> Result<([u8; 32], M), ()> {
    record.verify(domain).map_err(|_| ())?;
    let request = M::decode(record.payload.as_slice()).map_err(|_| ())?;
    if !check(&request) {
        return Err(());
    }
    let key: [u8; 32] = record.public_key.as_slice().try_into().map_err(|_| ())?;
    Ok((dyapp_identity::key_hash(&key), request))
}

/// Whether `got` is the nonce issued on this connection.
fn fresh(expected: Option<[u8; 32]>, got: &[u8]) -> bool {
    expected.is_some_and(|expected| got == expected)
}

fn mailbox_too_large(max: usize) -> MailboxResponse {
    MailboxResponse {
        max_bytes: max as u64,
        ..status(Status::TooLarge)
    }
}

fn status(status: Status) -> MailboxResponse {
    MailboxResponse {
        status: status.into(),
        ..MailboxResponse::default()
    }
}

fn profile_too_large(max: usize) -> ProfileResponse {
    ProfileResponse {
        max_bytes: max as u64,
        ..reply(Status::TooLarge, None)
    }
}

fn reply(status: Status, record: Option<SignedRecord>) -> ProfileResponse {
    ProfileResponse {
        status: status.into(),
        record,
        ..ProfileResponse::default()
    }
}

fn node_status(status: Status) -> NodeResponse {
    NodeResponse {
        status: status.into(),
        ..NodeResponse::default()
    }
}

/// TURN REST credentials as coturn's `use-auth-secret` checks them: username
/// `<expiry>:<peer>`, password base64(HMAC-SHA1(secret, username)).
fn turn_credentials(turn: &Turn, peer: &str, now: u64) -> TurnCredentials {
    use base64::Engine;
    use hmac::Mac;
    let expires = now + u64::from(turn.credential_minutes) * 60;
    let username = format!("{expires}:{peer}");
    let mut mac = hmac::Hmac::<sha1::Sha1>::new_from_slice(turn.secret.as_bytes())
        .expect("HMAC takes a key of any length");
    mac.update(username.as_bytes());
    TurnCredentials {
        password: base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()),
        username,
        urls: turn.urls.clone(),
        expires,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;

    fn assert_server_clock(server_time: u64, before: u64) {
        let after = chrono::Utc::now().timestamp().unsigned_abs();
        assert!(
            (before..=after).contains(&server_time),
            "{before} <= {server_time} <= {after}"
        );
    }

    #[test]
    fn info_and_heartbeat_time_failures_report_clock_without_changing_gossip() {
        let s = service();
        let search = search_service();
        let owner = Identity::generate();
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let info = s.node(
            &peer("clock"),
            NodeRequest {
                request: Some(node_request::Request::Info(proto::InfoRequest {})),
            },
        );
        assert_eq!(info.status, i32::from(Status::Ok));
        assert_server_clock(info.info.unwrap().server_time, now);
        for time in [0, now + 3600, u64::MAX] {
            let record = owner.sign(
                Domain::Heartbeat,
                proto::Heartbeat { time, version: 1 }.encode_to_vec(),
            );
            let before = chrono::Utc::now().timestamp().unsigned_abs();
            let response = s
                .profile(
                    &peer("clock"),
                    ProfileRequest {
                        request: Some(profile_request::Request::Heartbeat(record.clone())),
                    },
                )
                .unwrap();
            assert_eq!(response.status, i32::from(Status::Invalid));
            assert_server_clock(response.server_time, before);
            let response = search_as(
                &search,
                &peer("clock"),
                search_request::Request::Heartbeat(record.clone()),
            );
            assert_eq!(response.status, i32::from(Status::Invalid));
            assert_server_clock(response.server_time, before);
            let gossip = SearchRequest {
                request: Some(search_request::Request::Heartbeat(record)),
            };
            assert_eq!(
                search.search_gossip(&gossip.encode_to_vec()),
                Verdict::Ignore
            );
        }
        let record = owner.sign(
            Domain::Heartbeat,
            proto::Heartbeat {
                time: chrono::Utc::now().timestamp().unsigned_abs(),
                version: 1,
            }
            .encode_to_vec(),
        );
        let response = s
            .profile(
                &peer("clock"),
                ProfileRequest {
                    request: Some(profile_request::Request::Heartbeat(record.clone())),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::NotFound), 0)
        );
        let response = search_as(
            &search,
            &peer("clock"),
            search_request::Request::Heartbeat(record.clone()),
        );
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::NotFound), 0)
        );
        let mut forged = record;
        forged.payload.push(0);
        let response = s
            .profile(
                &peer("clock"),
                ProfileRequest {
                    request: Some(profile_request::Request::Heartbeat(forged.clone())),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Denied), 0)
        );
        let response = search_as(
            &search,
            &peer("clock"),
            search_request::Request::Heartbeat(forged),
        );
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Denied), 0)
        );
        let response = s.profile(&peer("clock"), get(vec![])).unwrap();
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Invalid), 0)
        );
    }

    #[test]
    fn media_time_hints_cover_direct_replica_and_shard_authorizations() {
        use proto::media_replica_put::Authorization;
        let s = media_service(|l| l.media_requests_per_second = 100);
        let from = replica_peer("clock");
        let owner = Identity::generate();
        let data = b"clock".to_vec();
        let hash = dyapp_identity::sha256(&data).to_vec();
        let (manifest, shards) = crate::replication::media_encode(&data).unwrap();
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        for (keep, time) in [
            (true, 0),
            (true, now + 3600),
            (true, u64::MAX),
            (false, 0),
            (false, now + 3600),
            (false, u64::MAX),
        ] {
            let authorization = if keep {
                Authorization::Keep(
                    owner.sign(
                        Domain::MediaKeep,
                        proto::MediaKeep {
                            version: 1,
                            hashes: vec![hash.clone()],
                            time,
                        }
                        .encode_to_vec(),
                    ),
                )
            } else {
                Authorization::Attach(
                    owner.sign(
                        Domain::MediaAttach,
                        proto::MediaAttach {
                            release: dyapp_identity::sha256(b"clock-release").to_vec(),
                            hashes: vec![hash.clone()],
                            created: time,
                        }
                        .encode_to_vec(),
                    ),
                )
            };
            let direct = match &authorization {
                Authorization::Keep(record) => media_request::Request::Keep(record.clone()),
                Authorization::Attach(record) => media_request::Request::Attach(record.clone()),
            };
            let replica = proto::MediaReplicaPut {
                data: data.clone(),
                authorization: Some(authorization.clone()),
                manifest: None,
            };
            let shard = proto::MediaShardPut {
                replica: Some(proto::MediaReplicaPut {
                    manifest: Some(manifest.clone()),
                    authorization: Some(authorization),
                    data: vec![],
                }),
                index: 0,
                data: shards[0].clone(),
            };
            for (path, request) in [
                direct,
                media_request::Request::ReplicaPut(replica),
                media_request::Request::ShardPut(shard),
            ]
            .into_iter()
            .enumerate()
            {
                let before = chrono::Utc::now().timestamp().unsigned_abs();
                let response = s
                    .media(
                        &from,
                        MediaRequest {
                            request: Some(request),
                        },
                    )
                    .unwrap();
                if !keep && time == 0 && path == 0 {
                    // Direct attach has no past-freshness gate; expiry is cleaned up later.
                    assert_eq!(
                        (response.status, response.server_time),
                        (i32::from(Status::Ok), 0)
                    );
                } else {
                    assert_eq!(response.status, i32::from(Status::Invalid));
                    assert_server_clock(response.server_time, before);
                }
            }
        }
    }

    #[test]
    fn media_non_clock_errors_and_old_replica_keeps_preserve_existing_rules() {
        let s = media_service(|l| l.media_requests_per_second = 100);
        let from = replica_peer("clock-errors");
        let owner = Identity::generate();
        let data = b"old-keep".to_vec();
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let mut keep = proto::MediaKeep {
            version: 1,
            hashes: vec![dyapp_identity::sha256(&data).to_vec()],
            time: now - 3600,
        };
        let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
        let replica = proto::MediaReplicaPut {
            data,
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                record.clone(),
            )),
            manifest: None,
        };
        let response = s.store_media(&replica).unwrap();
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Ok), 0)
        );
        let response = s
            .media(
                &from,
                MediaRequest {
                    request: Some(media_request::Request::Keep(record)),
                },
            )
            .unwrap();
        assert_eq!(response.status, i32::from(Status::Invalid));
        assert_server_clock(response.server_time, now);
        keep.version = 0;
        keep.time = u64::MAX;
        let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
        let response = s
            .media(
                &from,
                MediaRequest {
                    request: Some(media_request::Request::Keep(record.clone())),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Invalid), 0)
        );
        let malformed = proto::MediaReplicaPut {
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                record.clone(),
            )),
            ..replica
        };
        let response = s.store_media(&malformed).unwrap();
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Invalid), 0)
        );
        let mut forged = record;
        forged.payload.push(0);
        let response = s
            .media(
                &from,
                MediaRequest {
                    request: Some(media_request::Request::Keep(forged)),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.server_time),
            (i32::from(Status::Denied), 0)
        );
    }

    #[test]
    fn response_hints_decode_legacy_messages_and_keep_wire_tags() {
        let legacy = [0x08, Status::TooLarge as u8];
        let mut profile = ProfileResponse::decode(legacy.as_slice()).unwrap();
        let mut mailbox = MailboxResponse::decode(legacy.as_slice()).unwrap();
        let mut media = MediaResponse::decode(legacy.as_slice()).unwrap();
        let mut search = SearchResponse::decode(legacy.as_slice()).unwrap();
        assert_eq!(profile.max_bytes, 0);
        assert_eq!(mailbox.max_bytes, 0);
        assert_eq!(media.max_bytes, 0);
        assert_eq!((search.max_bytes, search.required_pow_bits), (0, 0));

        profile.max_bytes = 100;
        mailbox.max_bytes = 100;
        media.max_bytes = 100;
        search.max_bytes = 100;
        search.required_pow_bits = 11;
        let wire = |fields: &[u8]| [legacy.as_slice(), fields].concat();
        assert_eq!(profile.encode_to_vec(), wire(&[0x20, 100]));
        assert_eq!(mailbox.encode_to_vec(), wire(&[0x30, 100]));
        assert_eq!(media.encode_to_vec(), wire(&[0x28, 100]));
        assert_eq!(search.encode_to_vec(), wire(&[0x40, 100, 0x48, 11]));
        assert_eq!(
            ProfileResponse::decode(profile.encode_to_vec().as_slice()).unwrap(),
            profile
        );
        assert_eq!(
            MailboxResponse::decode(mailbox.encode_to_vec().as_slice()).unwrap(),
            mailbox
        );
        assert_eq!(
            MediaResponse::decode(media.encode_to_vec().as_slice()).unwrap(),
            media
        );
        assert_eq!(
            SearchResponse::decode(search.encode_to_vec().as_slice()).unwrap(),
            search
        );
    }

    #[test]
    fn profile_inventory_returns_only_version_and_validates_key_and_budget() {
        let s = service_with(|limits| limits.bytes_per_second = 10_000);
        let owner = Identity::generate();
        let record = Profile {
            version: 2,
            ..Default::default()
        }
        .sign(&owner);
        s.store_profile(&record).unwrap();
        let inventory = |key| ProfileRequest {
            request: Some(profile_request::Request::Inventory(proto::GetProfile {
                peer_id: key,
            })),
        };
        let key = dyapp_identity::key_hash(&owner.public_key()).to_vec();
        let response = s
            .profile(&peer("inventory"), inventory(key.clone()))
            .unwrap();
        assert_eq!(status(&response), Status::Ok);
        assert_eq!(response.version, 2);
        assert!(response.record.is_none());
        assert_eq!(
            status(&s.profile(&peer("missing"), inventory(vec![0; 32])).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&s.profile(&peer("bad"), inventory(vec![0; 31])).unwrap()),
            Status::Invalid
        );
        s.traffic.add(s.config.limits.bytes_per_second);
        assert_eq!(
            status(&s.profile(&peer("busy"), inventory(key)).unwrap()),
            Status::RateLimited
        );
    }

    fn search_service() -> Service {
        let mut s = service_with(|limits| limits.bytes_per_second = 1 << 20);
        s.config.roles.push(Role::Search);
        s.config.search.min_profile_pow_bits = 2;
        let index = SearchIndex::open(&s.config.storage.search_path(), s.config.search.max_bytes);
        s.search = Some(Arc::new(index.unwrap()));
        s
    }

    /// A profile carrying a solved proof of work for `bits`.
    fn proven(id: &Identity, mut profile: Profile, bits: u32) -> SignedRecord {
        profile.pow_nonce = dyapp_profile::solve(&id.public_key(), bits, 0, 100_000).unwrap();
        profile.pow_bits = bits;
        profile.sign(id)
    }

    /// Applies a gossip message, running its proof-of-work check inline as the node does off
    /// the loop.
    fn gossip(s: &Service, data: &[u8]) -> Verdict {
        match s.search_gossip(data) {
            Verdict::Pow(check) if s.pow.check(&check) => s.search_gossip(data),
            Verdict::Pow(_) => Verdict::Reject,
            verdict => verdict,
        }
    }

    /// Runs deferred work inline, as the node does in `spawn_blocking`.
    fn search_as(s: &Service, peer: &Peer, request: search_request::Request) -> SearchResponse {
        let index = s.search.clone().unwrap();
        let request = SearchRequest {
            request: Some(request),
        };
        match s.search(peer, request).unwrap() {
            Search::Reply(reply) => reply,
            Search::Query(q) => run_query(&index, &q, s.config.search.max_scan_rows).unwrap(),
            Search::Get(keys) => {
                let reply = run_get(&index, &keys).unwrap();
                let bytes = reply.records.iter().map(Message::encoded_len).sum();
                if s.charge_get(peer, bytes) {
                    reply
                } else {
                    search_reply(Status::RateLimited)
                }
            }
            Search::Pow(check, record) => {
                let ok = s.pow.check(&check);
                s.search_pow_done(peer, &record, ok).unwrap()
            }
        }
    }

    fn search(s: &Service, request: search_request::Request) -> SearchResponse {
        search_as(s, &peer("p"), request)
    }

    fn search_status(response: &SearchResponse) -> Status {
        Status::try_from(response.status).unwrap()
    }

    fn beat(identity: &Identity, version: u64, time: u64) -> SignedRecord {
        let beat = proto::Heartbeat { time, version };
        identity.sign(Domain::Heartbeat, beat.encode_to_vec())
    }

    #[test]
    fn search_publish_query_get() {
        let s = search_service();
        let id = Identity::generate();
        let record = proven(
            &id,
            Profile {
                version: 1,
                age: 33,
                country: "DE".into(),
                ..Profile::default()
            },
            2,
        );
        let r = search(&s, search_request::Request::Publish(record.clone()));
        assert_eq!(search_status(&r), Status::Ok);
        let newer = proven(
            &id,
            Profile {
                version: 2,
                ..Profile::default()
            },
            2,
        );
        let r = search(&s, search_request::Request::Publish(newer));
        assert_eq!(search_status(&r), Status::RateLimited);
        assert!(r.retry_after > 0 && r.retry_after <= 600);
        let q = proto::SearchQuery {
            conditions: vec![proto::Condition {
                field: 3,
                op: proto::Op::Eq.into(),
                ints: vec![33],
                strings: vec![],
            }],
            limit: 0,
        };
        let r = search(&s, search_request::Request::Query(q));
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.applied, vec![3]);
        assert_eq!(r.index_version, crate::search::INDEX_VERSION);
        let bad = proto::SearchQuery {
            conditions: vec![proto::Condition::default(); 17],
            limit: 0,
        };
        let r = search(&s, search_request::Request::Query(bad));
        assert_eq!(search_status(&r), Status::Invalid);
        let get = |keys| search_request::Request::Get(proto::SearchGet { keys });
        let key = r_key(&s, 33);
        let r = search(&s, get(vec![key.clone()]));
        assert_eq!(r.records, vec![record]);
        assert_eq!(
            search_status(&search(&s, get(vec![vec![0; 31]]))),
            Status::Invalid
        );
        assert_eq!(
            search_status(&search(&s, get(vec![key; 51]))),
            Status::Invalid
        );
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let r = search(&s, search_request::Request::Heartbeat(beat(&id, 1, now)));
        assert_eq!(search_status(&r), Status::Ok);
        let r = search(
            &s,
            search_request::Request::Heartbeat(beat(&id, 1, now - 3_600)),
        );
        assert_eq!(search_status(&r), Status::Invalid);
        let info = s.info();
        assert!(info.roles.contains(&proto::Role::Search.into()));
        assert_eq!(info.search_fields, SEARCH_FIELDS);
        assert_eq!(info.max_search_profile_bytes, 64 << 10);
        assert_eq!(info.min_profile_pow, 2);
        assert_eq!(service().info().min_profile_pow, 0);
        assert!(service().info().search_fields.is_empty());
    }

    /// The key of the one profile aged `age`.
    fn r_key(s: &Service, age: i64) -> Vec<u8> {
        let q = proto::SearchQuery {
            conditions: vec![proto::Condition {
                field: 3,
                op: proto::Op::Eq.into(),
                ints: vec![age],
                strings: vec![],
            }],
            limit: 0,
        };
        search_as(s, &peer("key"), search_request::Request::Query(q)).hits[0]
            .key
            .clone()
    }

    #[test]
    fn search_network_accepts_records_above_local_send_limit_and_bounds_the_network() {
        let mut s = search_service();
        s.config.search.max_profile_bytes = 4096;
        s.config.search.min_profile_pow_bits = 0;
        let owner = Identity::generate();
        let profile = Profile {
            version: 1,
            age: 28,
            ..Default::default()
        };
        let record = padded_profile(&owner, &profile, 8192);
        let response = search(&s, search_request::Request::Publish(record.clone()));
        assert_eq!(
            (
                response.status,
                response.max_bytes,
                response.required_pow_bits
            ),
            (i32::from(Status::TooLarge), 4096, 0)
        );
        let response = s.search_pow_done(&peer("key"), &record, true).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), 4096)
        );
        let wrap = |record| {
            SearchRequest {
                request: Some(search_request::Request::Publish(record)),
            }
            .encode_to_vec()
        };
        assert_eq!(gossip(&s, &wrap(record)), Verdict::Accept);
        let newer = padded_profile(
            &owner,
            &Profile {
                version: 2,
                ..profile.clone()
            },
            8192,
        );
        assert_eq!(s.search_apply(&newer).unwrap(), Caught::Stored);
        let profile = Profile {
            version: 3,
            ..profile
        };
        let over = padded_profile(&owner, &profile, SEARCH_ACCEPT_BYTES);
        assert!(over.encoded_len() > SEARCH_ACCEPT_BYTES);
        assert_eq!(gossip(&s, &wrap(over.clone())), Verdict::Ignore);
        assert_eq!(s.search_apply(&over).unwrap(), Caught::Dropped);
        let overhead = over.encoded_len() - over.payload.len();
        let exact = padded_profile(&owner, &profile, SEARCH_ACCEPT_BYTES - overhead);
        assert_eq!(exact.encoded_len(), SEARCH_ACCEPT_BYTES);
        assert_eq!(s.search_apply(&exact).unwrap(), Caught::Stored);
    }

    #[test]
    fn low_pow_heartbeat_does_not_refresh_or_remove_the_stored_record() {
        let mut s = search_service();
        s.config.search.min_profile_pow_bits = 0;
        let owner = Identity::generate();
        let record = Profile {
            version: 1,
            ..Default::default()
        }
        .sign(&owner);
        assert_eq!(
            search(&s, search_request::Request::Publish(record.clone())).status,
            i32::from(Status::Ok)
        );
        let key = dyapp_identity::key_hash(&owner.public_key());
        let index = s.search.clone().unwrap();
        let counters = || {
            index
                .reader()
                .unwrap()
                .query_row(
                    "SELECT `heartbeat`, `beat_day`, `beats` FROM `key` WHERE `key` = ?",
                    [&key[..]],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .unwrap()
        };
        let before = counters();
        s.config.search.min_profile_pow_bits = 2;
        let heartbeat = beat(&owner, 1, chrono::Utc::now().timestamp().unsigned_abs());
        let from = peer("weak-heartbeat");
        let response = search_as(
            &s,
            &from,
            search_request::Request::Heartbeat(heartbeat.clone()),
        );
        assert_eq!(
            (
                response.status,
                response.required_pow_bits,
                response.server_time
            ),
            (i32::from(Status::Invalid), 2, 0)
        );
        assert!(!s.reputation.banned(&from.id));
        let data = SearchRequest {
            request: Some(search_request::Request::Heartbeat(heartbeat.clone())),
        }
        .encode_to_vec();
        assert_eq!(s.search_gossip(&data), Verdict::Ignore);
        assert_eq!(counters(), before);
        assert_eq!(index.records(&[key], usize::MAX).unwrap(), vec![record]);
        let unknown = Identity::generate();
        let response = search(
            &s,
            search_request::Request::Heartbeat(beat(
                &unknown,
                1,
                chrono::Utc::now().timestamp().unsigned_abs(),
            )),
        );
        assert_eq!(
            (response.status, response.required_pow_bits),
            (i32::from(Status::NotFound), 0)
        );
        let strong = Identity::generate();
        let record = proven(
            &strong,
            Profile {
                version: 1,
                ..Default::default()
            },
            2,
        );
        assert_eq!(
            search(&s, search_request::Request::Publish(record)).status,
            i32::from(Status::Ok)
        );
        assert_eq!(
            search(
                &s,
                search_request::Request::Heartbeat(beat(
                    &strong,
                    1,
                    chrono::Utc::now().timestamp().unsigned_abs()
                ))
            )
            .status,
            i32::from(Status::Ok)
        );
        s.config.search.min_profile_pow_bits = 0;
        assert_eq!(
            search(&s, search_request::Request::Heartbeat(heartbeat)).status,
            i32::from(Status::Ok)
        );
        assert_eq!(counters().2, before.2 + 1);
    }

    #[test]
    fn search_pow_hint_is_specific_to_insufficient_difficulty() {
        let s = search_service();
        let owner = Identity::generate();
        let weak = Profile {
            version: 1,
            ..Default::default()
        }
        .sign(&owner);
        let response = search(&s, search_request::Request::Publish(weak.clone()));
        assert_eq!(
            (
                response.status,
                response.required_pow_bits,
                response.server_time
            ),
            (
                i32::from(Status::Invalid),
                s.config.search.min_profile_pow_bits,
                0
            )
        );
        let malformed = Profile::default().sign(&owner);
        let response = search(&s, search_request::Request::Publish(malformed));
        assert_eq!(
            (
                response.status,
                response.required_pow_bits,
                response.max_bytes
            ),
            (i32::from(Status::Invalid), 0, 0)
        );
        let mut forged = weak;
        forged.payload.push(0);
        let response = search(&s, search_request::Request::Publish(forged));
        assert_eq!(
            (response.status, response.required_pow_bits),
            (i32::from(Status::Denied), 0)
        );
    }

    #[test]
    fn search_refusals_and_gossip() {
        let unsupported = service()
            .search(&peer("p"), SearchRequest::default())
            .unwrap();
        assert!(
            matches!(unsupported, Search::Reply(r) if search_status(&r) == Status::Unsupported)
        );
        let s = search_service();
        let id = Identity::generate();
        let mut forged = Profile {
            version: 1,
            ..Profile::default()
        }
        .sign(&id);
        forged.payload.push(0);
        let r = search(&s, search_request::Request::Publish(forged.clone()));
        assert_eq!(search_status(&r), Status::Denied);
        let big = Profile {
            version: 1,
            place: "x".repeat(1000),
            interests: vec!["y".repeat(900); 80],
            ..Profile::default()
        }
        .sign(&id);
        let r = search(&s, search_request::Request::Publish(big));
        assert_eq!(search_status(&r), Status::TooLarge);
        let wrap = |r: SignedRecord| {
            SearchRequest {
                request: Some(search_request::Request::Publish(r)),
            }
            .encode_to_vec()
        };
        assert_eq!(gossip(&s, &wrap(forged)), Verdict::Reject);
        assert_eq!(gossip(&s, b"junk"), Verdict::Reject);
        let good = proven(
            &id,
            Profile {
                version: 1,
                ..Profile::default()
            },
            2,
        );
        assert_eq!(gossip(&s, &wrap(good.clone())), Verdict::Accept);
        assert_eq!(gossip(&s, &wrap(good)), Verdict::Ignore);
        s.traffic.add(s.config.limits.bytes_per_second);
        let q = proto::SearchQuery::default();
        let r = search(&s, search_request::Request::Query(q));
        assert_eq!(search_status(&r), Status::RateLimited);
    }

    #[test]
    fn search_profile_pow() {
        // One strike bans, so both a missing strike and a given one are visible.
        let mut s = service_with(|l| {
            l.bytes_per_second = 1 << 20;
            l.strikes_to_ban = 1;
        });
        s.config.roles.push(Role::Search);
        s.config.search.min_profile_pow_bits = 2;
        let index = SearchIndex::open(&s.config.storage.search_path(), s.config.search.max_bytes);
        s.search = Some(Arc::new(index.unwrap()));
        let id = Identity::generate();
        let v1 = || Profile {
            version: 1,
            ..Profile::default()
        };
        let publish = |who: &str, r: SignedRecord| {
            search_status(&search_as(
                &s,
                &peer(who),
                search_request::Request::Publish(r),
            ))
        };
        // Below the minimum: INVALID, no strike.
        assert_eq!(publish("weak", proven(&id, v1(), 1)), Status::Invalid);
        assert!(!s.reputation.banned("weak"));
        // A 20-bit claim at nonce 0 holds with probability 2^-20: DENIED and a strike.
        let forged = Profile {
            pow_bits: 20,
            ..v1()
        };
        assert_eq!(publish("liar", forged.sign(&id)), Status::Denied);
        assert!(s.reputation.banned("liar"));
        // Valid: OK; the tombstone keeping the same proof needs no Argon2.
        let good = proven(&id, v1(), 2);
        assert_eq!(publish("good", good.clone()), Status::Ok);
        let (checks, _) = s.pow.take_counts();
        assert!(checks >= 1);
        let nonce = dyapp_profile::verify(&good).unwrap().profile.pow_nonce;
        let tomb = Profile::tombstone(2, nonce, 2).sign(&id);
        assert_eq!(s.search_apply(&tomb).unwrap(), Caught::Stored);
        assert_eq!(s.pow.take_counts(), (0, 0));
        // Gossip: below the minimum is ignored, a forged proof rejected.
        let wrap = |r: SignedRecord| {
            SearchRequest {
                request: Some(search_request::Request::Publish(r)),
            }
            .encode_to_vec()
        };
        let other = Identity::generate();
        assert_eq!(s.search_gossip(&wrap(v1().sign(&other))), Verdict::Ignore);
        let lie = Profile {
            pow_bits: 20,
            ..v1()
        };
        assert_eq!(gossip(&s, &wrap(lie.sign(&other))), Verdict::Reject);
        assert_eq!(gossip(&s, &wrap(proven(&other, v1(), 2))), Verdict::Accept);
    }

    #[test]
    fn search_peers_sync_on_their_own_budget() {
        let s = search_service();
        let id = Identity::generate();
        let record = proven(
            &id,
            Profile {
                version: 1,
                ..Profile::default()
            },
            2,
        );
        search(&s, search_request::Request::Publish(record));
        let newer = proven(
            &id,
            Profile {
                version: 2,
                age: 40,
                ..Profile::default()
            },
            2,
        );
        // Inside 10 minutes: catch-up skips the update limit.
        assert_eq!(s.search_apply(&newer).unwrap(), Caught::Stored);
        let key = search_key(&SearchRequest {
            request: Some(search_request::Request::Publish(newer.clone())),
        })
        .unwrap();
        let sync = peer("sync");
        s.search_peer(&sync.id, true);
        let get = |peer: &Peer| {
            let keys = vec![key.to_vec()];
            search_as(
                &s,
                peer,
                search_request::Request::Get(proto::SearchGet { keys }),
            )
        };
        // Far past the client budget of `requests_per_second`.
        for _ in 0..1_000 {
            assert_eq!(get(&sync).records, vec![newer.clone()]);
        }
        let client = peer("client");
        assert!((0..1_000).any(|_| search_status(&get(&client)) == Status::RateLimited));
        let inventory = |peer: &Peer, topic| {
            let request = InventoryRequest {
                topic,
                after: vec![],
            };
            s.search_inventory(peer, request).unwrap()
        };
        let have = inventory(&sync, crate::search::topic(&key)).have;
        assert_eq!(have[0].key, key.to_vec());
        assert_eq!(status_of(inventory(&sync, 16).status), Status::Invalid);
        s.search_sync
            .add(u64::from(s.config.maintenance.io_mb_per_s) << 20);
        assert_eq!(search_status(&get(&sync)), Status::RateLimited);
        assert_eq!(status_of(inventory(&sync, 0).status), Status::RateLimited);
        s.search_peer(&sync.id, false);
        assert!(!s.search_peers.lock().unwrap().contains(&sync.id));
    }

    fn status_of(status: i32) -> Status {
        Status::try_from(status).unwrap()
    }

    fn service() -> Service {
        service_with(|_| {})
    }

    fn peer(id: &str) -> Peer {
        Peer {
            id: id.into(),
            group: "g".into(),
        }
    }

    fn replica_peer(label: &str) -> Peer {
        let key = dyapp_identity::sha256(label.as_bytes());
        let id = libp2p::identity::Keypair::ed25519_from_bytes(key)
            .unwrap()
            .public()
            .to_peer_id()
            .to_string();
        Peer {
            id,
            group: "g".into(),
        }
    }

    fn service_with(change: impl FnOnce(&mut crate::config::Limits)) -> Service {
        let dir = format!("/tmp/ai/test-service-{}", uuid::Uuid::new_v4());
        let mut config = NodeConfig::default();
        config.storage.dir = dir.clone().into();
        config.network.id_pow_bits = 0;
        change(&mut config.limits);
        Service::new(BootstrapStore::new(&dir).unwrap(), config)
    }

    fn publish(record: SignedRecord) -> ProfileRequest {
        ProfileRequest {
            request: Some(profile_request::Request::Publish(record)),
        }
    }

    fn get(peer_id: Vec<u8>) -> ProfileRequest {
        ProfileRequest {
            request: Some(profile_request::Request::Get(proto::GetProfile { peer_id })),
        }
    }

    fn status(response: &ProfileResponse) -> Status {
        Status::try_from(response.status).unwrap()
    }

    /// A signed future-compatible profile of an exact payload size.
    fn padded_profile(owner: &Identity, profile: &Profile, size: usize) -> SignedRecord {
        padded_record(owner, Domain::Profile, profile.encode_to_vec(), size)
    }

    fn padded_record(
        owner: &Identity,
        domain: Domain,
        mut payload: Vec<u8>,
        size: usize,
    ) -> SignedRecord {
        #[derive(Clone, PartialEq, prost::Message)]
        struct Extension {
            #[prost(bytes = "vec", tag = "100")]
            data: Vec<u8>,
        }
        let mut extension = Extension {
            data: vec![0; size],
        };
        let overhead = extension.encoded_len() - extension.data.len();
        extension.data.resize(size - payload.len() - overhead, 0);
        payload.extend(extension.encode_to_vec());
        assert_eq!(payload.len(), size);
        owner.sign(domain, payload)
    }

    #[test]
    fn profile_size_hints_cover_send_replica_and_local_boundaries() {
        let s = service();
        let owner = Identity::generate();
        let profile = Profile {
            version: 1,
            age: 28,
            ..Default::default()
        };
        let exact = padded_profile(&owner, &profile, PROFILE_SEND_BYTES);
        assert!(dyapp_profile::verify(&exact).is_ok());
        let response = s.publish(&exact, false, PROFILE_SEND_BYTES - 1).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), (PROFILE_SEND_BYTES - 1) as u64)
        );
        let response = s
            .profile(&peer("profile-limit"), publish(exact.clone()))
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::Ok), 0)
        );
        let response = s.store_profile(&exact).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::Stale), 0)
        );
        let from = replica_peer("profile-limit-node");
        let response = s
            .profile(
                &from,
                ProfileRequest {
                    request: Some(profile_request::Request::ReplicaPut(exact)),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::Stale), 0)
        );
        let large = padded_profile(&owner, &profile, PROFILE_SEND_BYTES + 1);
        let response = s
            .profile(&peer("profile-limit"), publish(large.clone()))
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), PROFILE_SEND_BYTES as u64)
        );
        let response = s.store_profile(&large).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), PROFILE_ACCEPT_BYTES as u64)
        );
        let response = s
            .profile(
                &from,
                ProfileRequest {
                    request: Some(profile_request::Request::ReplicaPut(large)),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), PROFILE_ACCEPT_BYTES as u64)
        );
    }

    #[test]
    fn publish_get_stale_and_errors() {
        let service = service();
        let identity = Identity::generate();
        let hex_id = identity.peer_id();
        let id: Vec<u8> = (0..hex_id.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_id[i..i + 2], 16).unwrap())
            .collect();
        let v2 = Profile {
            version: 2,
            age: 30,
            ..Profile::default()
        }
        .sign(&identity);

        assert_eq!(
            status(&service.profile(&peer("p"), get(id.clone())).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&service.profile(&peer("p"), publish(v2.clone())).unwrap()),
            Status::Ok
        );
        let got = service.profile(&peer("p"), get(id.clone())).unwrap();
        assert_eq!((status(&got), got.record), (Status::Ok, Some(v2.clone())));

        let v1 = Profile {
            version: 1,
            ..Profile::default()
        }
        .sign(&identity);
        let stale = service.profile(&peer("p"), publish(v1)).unwrap();
        assert_eq!(
            (status(&stale), stale.record),
            (Status::Stale, Some(v2.clone()))
        );

        let mut forged = v2;
        forged.payload.push(0);
        assert_eq!(
            status(&service.profile(&peer("p"), publish(forged)).unwrap()),
            Status::Denied
        );
        assert_eq!(
            status(&service.profile(&peer("p"), get(vec![1; 5])).unwrap()),
            Status::Invalid
        );
        let empty = ProfileRequest { request: None };
        assert_eq!(
            status(&service.profile(&peer("p"), empty).unwrap()),
            Status::Unsupported
        );
    }

    fn replica_status(service: &Service, sender: &Peer, protocol: u8) -> Status {
        let status = match protocol {
            0 => {
                service
                    .mailbox(
                        sender,
                        &mut None,
                        MailboxRequest {
                            request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                                envelopes: vec![SignedRecord::default()],
                            })),
                        },
                    )
                    .unwrap()
                    .status
            }
            1 => {
                service
                    .profile(
                        sender,
                        ProfileRequest {
                            request: Some(profile_request::Request::ReplicaPut(
                                SignedRecord::default(),
                            )),
                        },
                    )
                    .unwrap()
                    .status
            }
            2 => {
                service
                    .media(
                        sender,
                        MediaRequest {
                            request: Some(media_request::Request::ReplicaPut(
                                proto::MediaReplicaPut {
                                    data: vec![0],
                                    ..Default::default()
                                },
                            )),
                        },
                    )
                    .unwrap()
                    .status
            }
            3 => {
                service
                    .media(
                        sender,
                        MediaRequest {
                            request: Some(media_request::Request::ShardPut(proto::MediaShardPut {
                                data: vec![0],
                                ..Default::default()
                            })),
                        },
                    )
                    .unwrap()
                    .status
            }
            _ => unreachable!(),
        };
        Status::try_from(status).unwrap()
    }

    #[test]
    fn replicas_reject_missing_pow_and_bad_signatures_with_strikes() {
        let valid = dyapp_p2p_net::generate_pow_keypair(8).public().to_peer_id();
        let invalid = loop {
            let id = libp2p::PeerId::random();
            if !dyapp_p2p_net::id_has_pow(&id, 8) {
                break id;
            }
        };
        for protocol in 0..4 {
            for sender in [
                peer("not-a-peer"),
                peer(&invalid.to_string()),
                peer(&valid.to_string()),
            ] {
                let mut service = media_service(|l| l.strikes_to_ban = 1);
                service.config.network.id_pow_bits = 8;
                assert_eq!(replica_status(&service, &sender, protocol), Status::Denied);
                assert!(service.reputation.banned(&sender.id), "protocol {protocol}");
            }
        }
    }

    #[test]
    fn shard_upload_requires_node_pow_and_accepts_authorised_node() {
        let mut service = media_service(|_| {});
        service.config.network.id_pow_bits = 8;
        let owner = Identity::generate();
        let (manifest, shards) = crate::replication::media_encode(&vec![1; (1 << 20) + 1]).unwrap();
        let hash = manifest.hash.clone();
        let keep = proto::MediaKeep {
            version: 1,
            hashes: vec![hash.clone()],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        };
        let put = proto::MediaShardPut {
            replica: Some(proto::MediaReplicaPut {
                manifest: Some(manifest),
                authorization: Some(proto::media_replica_put::Authorization::Keep(
                    owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
                )),
                ..Default::default()
            }),
            data: shards[0].clone(),
            index: 0,
        };
        let request = || MediaRequest {
            request: Some(media_request::Request::ShardPut(put.clone())),
        };
        let weak = loop {
            let id = libp2p::PeerId::random();
            if !dyapp_p2p_net::id_has_pow(&id, 8) {
                break peer(&id.to_string());
            }
        };
        assert_eq!(
            service.media(&weak, request()).unwrap().status,
            i32::from(Status::Denied)
        );
        assert!(service
            .media
            .as_ref()
            .unwrap()
            .manifest(&hash)
            .unwrap()
            .is_none());
        let node = dyapp_p2p_net::generate_pow_keypair(8).public().to_peer_id();
        assert_eq!(
            service
                .media(&peer(&node.to_string()), request())
                .unwrap()
                .status,
            i32::from(Status::Ok)
        );
        assert!(service
            .media
            .as_ref()
            .unwrap()
            .manifest(&hash)
            .unwrap()
            .is_some());
    }

    #[test]
    fn replica_peer_group_and_media_rate_refusals_do_not_strike() {
        let sender = replica_peer("node");
        for protocol in 0..4 {
            for bucket in 0..4 {
                if bucket >= 2 && protocol < 2 {
                    continue;
                }
                let mut service = media_service(|l| {
                    l.strikes_to_ban = 1;
                    l.requests_per_second = 1;
                    l.ip_group_requests_per_second = 1;
                    l.media_requests_per_second = 1;
                    l.bytes_per_second = 1_000_000;
                });
                service.media_bytes = PeerRateLimiter::new(1);
                assert!(match bucket {
                    0 => service.rate_limiter.check_limit(&sender.id),
                    1 => service.groups.check_limit(&sender.group),
                    2 => service.media_peers.check_limit(&sender.id),
                    3 => service.media_bytes.check_limit(&sender.id),
                    _ => unreachable!(),
                });
                assert_eq!(
                    replica_status(&service, &sender, protocol),
                    Status::RateLimited,
                    "protocol {protocol}, bucket {bucket}"
                );
                assert!(
                    !service.reputation.banned(&sender.id),
                    "protocol {protocol}, bucket {bucket}"
                );
            }
        }
        // A plain client still receives a strike when the peer bucket refuses it.
        let service = service_with(|l| {
            l.strikes_to_ban = 1;
            l.requests_per_second = 1;
        });
        assert!(service.rate_limiter.check_limit("client"));
        assert_eq!(
            status(&service.profile(&peer("client"), get(vec![0; 32])).unwrap()),
            Status::RateLimited
        );
        assert!(service.reputation.banned("client"));
    }

    #[test]
    fn admission_stores_nothing_and_replicas_verify_and_cost_one_unit() {
        let identity = Identity::generate();
        let record = Profile {
            version: 1,
            ..Profile::default()
        }
        .sign(&identity);
        let service = service_with(|l| l.requests_per_second = 5);
        assert_eq!(
            status(
                &service
                    .prepare_profile(&peer("client"), record.clone())
                    .unwrap()
            ),
            Status::Ok
        );
        assert!(service
            .store
            .get_profile(&identity.peer_id())
            .unwrap()
            .is_none());
        assert_eq!(
            status(&service.profile(&peer("client"), get(vec![0; 32])).unwrap()),
            Status::RateLimited
        );
        let replica = |record| ProfileRequest {
            request: Some(profile_request::Request::ReplicaPut(record)),
        };
        assert_eq!(
            status(
                &service
                    .profile(&replica_peer("holder"), replica(record.clone()))
                    .unwrap()
            ),
            Status::Ok
        );
        assert_eq!(
            status(
                &service
                    .profile(&replica_peer("holder"), get(vec![0; 32]))
                    .unwrap()
            ),
            Status::NotFound
        );
        let mut forged = record;
        forged.signature[0] ^= 1;
        assert_eq!(
            status(
                &service
                    .profile(&replica_peer("holder"), replica(forged))
                    .unwrap()
            ),
            Status::Denied
        );

        let envelope = proto::Envelope {
            id: vec![3; 16],
            mailbox: vec![4; 32],
            ciphertext: vec![5],
        };
        let record = identity.sign(Domain::Envelope, envelope.encode_to_vec());
        assert_eq!(
            mailbox_status(
                &service
                    .prepare_mailbox(&peer("sender"), record.clone())
                    .unwrap()
            ),
            Status::Ok
        );
        assert!(service.held(&envelope.mailbox).unwrap().is_empty());
        assert_eq!(
            mailbox_status(&service.store_mailbox(&record).unwrap()),
            Status::Ok
        );
        assert_eq!(service.held(&envelope.mailbox).unwrap().len(), 1);
    }

    #[test]
    fn heartbeat_keeps_the_profile_seen() {
        let service = service();
        let identity = Identity::generate();
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let beat = |time, identity: &Identity| {
            let beat = proto::Heartbeat { time, version: 0 };
            ProfileRequest {
                request: Some(profile_request::Request::Heartbeat(
                    identity.sign(Domain::Heartbeat, beat.encode_to_vec()),
                )),
            }
        };
        let send = |request| status(&service.profile(&peer("p"), request).unwrap());
        assert_eq!(send(beat(now, &identity)), Status::NotFound);
        let profile = Profile {
            version: 1,
            age: 30,
            ..Profile::default()
        };
        assert_eq!(send(publish(profile.sign(&identity))), Status::Ok);
        let db =
            rusqlite::Connection::open(service.config.storage.dir.join("profiles.db")).unwrap();
        let seen = || {
            db.query_row(
                "SELECT `last_seen` FROM `profiles` WHERE `peer_id` = ?",
                [identity.peer_id()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        };
        let old = i64::try_from(now).unwrap() - 120;
        db.execute(
            "UPDATE `profiles` SET `last_seen` = ? WHERE `peer_id` = ?",
            rusqlite::params![old, identity.peer_id()],
        )
        .unwrap();
        // Neither the same version nor an older version can refresh retention.
        assert_eq!(send(publish(profile.sign(&identity))), Status::Stale);
        let newer = Profile {
            version: 2,
            ..profile.clone()
        };
        assert_eq!(send(publish(newer.sign(&identity))), Status::Ok);
        assert!(seen() >= i64::try_from(now).unwrap());
        db.execute(
            "UPDATE `profiles` SET `last_seen` = ? WHERE `peer_id` = ?",
            rusqlite::params![old, identity.peer_id()],
        )
        .unwrap();
        assert_eq!(send(publish(profile.sign(&identity))), Status::Stale);
        assert_eq!(seen(), old);
        // Store signed time, not receipt time, and never move it back on replay.
        assert_eq!(send(beat(now - 60, &identity)), Status::Ok);
        assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
        assert_eq!(send(beat(now - 60, &identity)), Status::Ok);
        assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
        assert_eq!(send(beat(now - 90, &identity)), Status::Ok);
        assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
        assert_eq!(send(beat(now - 3600, &identity)), Status::Invalid);
        assert_eq!(send(beat(now + 3600, &identity)), Status::Invalid);
        assert_eq!(send(beat(u64::MAX, &identity)), Status::Invalid);
        assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
        let mut forged = beat(now, &identity);
        if let Some(profile_request::Request::Heartbeat(record)) = &mut forged.request {
            record.payload.push(0);
        }
        assert_eq!(send(forged), Status::Denied);
        assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
        let cutoff = i64::try_from(now).unwrap() - 1;
        assert_eq!(service.store.expire_profiles(cutoff).unwrap(), 1);
    }

    fn mailbox(
        service: &Service,
        nonce: &mut Option<[u8; 32]>,
        request: mailbox_request::Request,
    ) -> MailboxResponse {
        let sender = if matches!(request, mailbox_request::Request::ReplicaPut(_)) {
            replica_peer("replica")
        } else {
            peer("p")
        };
        let request = MailboxRequest {
            request: Some(request),
        };
        service.mailbox(&sender, nonce, request).unwrap()
    }

    fn challenge(service: &Service, nonce: &mut Option<[u8; 32]>) -> Vec<u8> {
        let request = mailbox_request::Request::Challenge(proto::ChallengeRequest {});
        mailbox(service, nonce, request).nonce
    }

    fn fetch(device: &Identity, nonce: Vec<u8>) -> mailbox_request::Request {
        let fetch = proto::Fetch {
            nonce,
            ..proto::Fetch::default()
        };
        mailbox_request::Request::Fetch(device.sign(Domain::MailboxFetch, fetch.encode_to_vec()))
    }

    fn mailbox_status(response: &MailboxResponse) -> Status {
        Status::try_from(response.status).unwrap()
    }

    #[test]
    fn deny_list_refuses_without_strikes() {
        let mut service = service_with(|l| l.strikes_to_ban = 1);
        let (listed, device) = (Identity::generate(), Identity::generate());
        let address = dyapp_identity::key_hash(&device.public_key()).to_vec();
        let listed_id = dyapp_identity::key_hash(&listed.public_key()).to_vec();
        service.deny = [hex(&listed_id), hex(&address), "bad".into(), "g6".into()].into();
        let profile = |p: &str, request| status(&service.profile(&peer(p), request).unwrap());
        let record = Profile::default().sign(&listed);
        assert_eq!(profile("p", publish(record)), Status::Refused);
        assert_eq!(profile("p", get(listed_id)), Status::Refused);
        assert_eq!(profile("bad", get(vec![1; 32])), Status::Refused);
        let group = Peer {
            id: "q".into(),
            group: "g6".into(),
        };
        let response = service.profile(&group, get(vec![1; 32])).unwrap();
        assert_eq!(status(&response), Status::Refused);
        let envelope = proto::Envelope {
            id: vec![1; 16],
            mailbox: address,
            ciphertext: vec![1],
        };
        let to_listed = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
        let put = mailbox_request::Request::Put(to_listed);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut None, put)),
            Status::Refused
        );
        assert!(!service.reputation.banned("p"));
        assert_eq!(profile("p", get(vec![1; 32])), Status::NotFound);
    }

    #[test]
    fn inventory_names_gaps_and_replica_put_fills_them() {
        let service = service_with(|l| l.strikes_to_ban = 1);
        let sender = Identity::generate();
        let address = vec![7; 32];
        let signed = |id: u8| {
            let envelope = proto::Envelope {
                id: vec![id; 16],
                mailbox: address.clone(),
                ciphertext: vec![id],
            };
            sender.sign(Domain::Envelope, envelope.encode_to_vec())
        };
        let put = mailbox_request::Request::Put(signed(1));
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut None, put)),
            Status::Ok
        );
        let inventory = |ids: Vec<Vec<u8>>| {
            let request = mailbox_request::Request::Inventory(proto::Inventory {
                mailbox: address.clone(),
                ids,
            });
            mailbox(&service, &mut None, request)
        };
        let response = inventory(vec![vec![1; 16], vec![2; 16]]);
        assert_eq!(response.missing, [vec![2; 16]]);
        assert_eq!(
            mailbox_status(&inventory(vec![vec![2; 3]])),
            Status::Invalid
        );
        let batch = |envelopes| {
            let request = mailbox_request::Request::ReplicaPut(proto::Envelopes { envelopes });
            mailbox_status(&mailbox(&service, &mut None, request))
        };
        assert_eq!(batch(vec![signed(2)]), Status::Ok);
        assert!(inventory(vec![vec![2; 16]]).missing.is_empty());
        let mut forged = signed(3);
        forged.signature[0] ^= 1;
        assert_eq!(batch(vec![signed(4), forged]), Status::Denied);
        assert_eq!(service.held(&address).unwrap().len(), 3);
    }

    #[test]
    fn mailbox_size_hints_cover_boundaries_local_replicas_and_denied_batch_priority() {
        let s = service();
        let owner = Identity::generate();
        let mut envelope = proto::Envelope {
            id: vec![4; 16],
            mailbox: vec![7; 32],
            ciphertext: vec![0; MAX_ENVELOPE_BYTES],
        };
        let overhead = envelope.encoded_len() - envelope.ciphertext.len();
        envelope.ciphertext.resize(MAX_ENVELOPE_BYTES - overhead, 0);
        let payload = envelope.encode_to_vec();
        assert_eq!(payload.len(), MAX_ENVELOPE_BYTES);
        let exact = owner.sign(Domain::Envelope, payload);
        let response = s.put(&exact, false, false, MAX_ENVELOPE_BYTES - 1).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), (MAX_ENVELOPE_BYTES - 1) as u64)
        );
        let response = s
            .mailbox(
                &peer("limits"),
                &mut None,
                MailboxRequest {
                    request: Some(mailbox_request::Request::Put(exact.clone())),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::Ok), 0)
        );
        let response = s.store_mailbox(&exact).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::Ok), 0)
        );
        envelope.ciphertext.push(0);
        let large = owner.sign(Domain::Envelope, envelope.encode_to_vec());
        assert_eq!(large.payload.len(), MAX_ENVELOPE_BYTES + 1);
        let response = s
            .mailbox(
                &peer("limits"),
                &mut None,
                MailboxRequest {
                    request: Some(mailbox_request::Request::Put(large.clone())),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), MAX_ENVELOPE_BYTES as u64)
        );
        let response = s.store_mailbox(&large).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), ENVELOPE_ACCEPT_BYTES as u64)
        );
        let from = replica_peer("limit-node");
        let response = s
            .mailbox(
                &from,
                &mut None,
                MailboxRequest {
                    request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                        envelopes: vec![large.clone(), exact.clone()],
                    })),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), ENVELOPE_ACCEPT_BYTES as u64)
        );
        let mut forged = exact;
        forged.signature[0] ^= 1;
        for envelopes in [vec![large.clone(), forged.clone()], vec![forged, large]] {
            let response = s
                .mailbox(
                    &from,
                    &mut None,
                    MailboxRequest {
                        request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                            envelopes,
                        })),
                    },
                )
                .unwrap();
            assert_eq!(
                (response.status, response.max_bytes),
                (i32::from(Status::Denied), 0)
            );
        }
    }

    #[test]
    fn mailbox_put_fetch_ack_forged_and_replayed() {
        let service = service();
        let (device, sender, thief) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let address = dyapp_identity::key_hash(&device.public_key()).to_vec();
        let envelope = |id: Vec<u8>, ciphertext| proto::Envelope {
            id,
            mailbox: address.clone(),
            ciphertext,
        };
        let put = |envelope: proto::Envelope| {
            mailbox_request::Request::Put(sender.sign(Domain::Envelope, envelope.encode_to_vec()))
        };
        let (mut conn, mut other) = (None, None);

        let signed = sender.sign(
            Domain::Envelope,
            envelope(vec![7; 16], vec![1]).encode_to_vec(),
        );
        let response = mailbox(
            &service,
            &mut conn,
            mailbox_request::Request::Put(signed.clone()),
        );
        assert_eq!(mailbox_status(&response), Status::Ok);
        let mut forged = signed.clone();
        forged.payload.push(0);
        let response = mailbox(&service, &mut conn, mailbox_request::Request::Put(forged));
        assert_eq!(mailbox_status(&response), Status::Denied);
        let wrong_domain = sender.sign(Domain::Profile, signed.payload.clone());
        let response = mailbox(
            &service,
            &mut conn,
            mailbox_request::Request::Put(wrong_domain),
        );
        assert_eq!(mailbox_status(&response), Status::Denied);
        let response = mailbox(&service, &mut conn, put(envelope(vec![7; 5], vec![1])));
        assert_eq!(mailbox_status(&response), Status::Invalid);
        let large = envelope(vec![8; 16], vec![0; MAX_ENVELOPE_BYTES]);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, put(large))),
            Status::TooLarge
        );

        // No challenge yet, then a nonce issued on another connection.
        let response = mailbox(&service, &mut conn, fetch(&device, vec![0; 32]));
        assert_eq!(mailbox_status(&response), Status::Denied);
        let foreign = challenge(&service, &mut other);
        let response = mailbox(&service, &mut conn, fetch(&device, foreign));
        assert_eq!(mailbox_status(&response), Status::Denied);

        // A fetch signed as an ack, then the real fetch and its replay.
        let nonce = challenge(&service, &mut conn);
        let ack_signed = proto::Fetch {
            nonce: nonce.clone(),
            ..proto::Fetch::default()
        };
        let cross = device.sign(Domain::MailboxAck, ack_signed.encode_to_vec());
        let response = mailbox(&service, &mut conn, mailbox_request::Request::Fetch(cross));
        assert_eq!(mailbox_status(&response), Status::Denied);
        let nonce = challenge(&service, &mut conn);
        let request = fetch(&device, nonce);
        let response = mailbox(&service, &mut conn, request.clone());
        assert_eq!(
            (mailbox_status(&response), response.envelopes, response.more),
            (Status::Ok, vec![signed], false)
        );
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, request)),
            Status::Denied
        );

        // Another key reads and acks only its own, empty mailbox.
        let nonce = challenge(&service, &mut conn);
        let response = mailbox(&service, &mut conn, fetch(&thief, nonce));
        assert_eq!(
            (mailbox_status(&response), response.envelopes.len()),
            (Status::Ok, 0)
        );
        let ack = |key: &Identity, nonce| {
            let ack = proto::Ack {
                nonce,
                ids: vec![vec![7; 16]],
            };
            mailbox_request::Request::Ack(key.sign(Domain::MailboxAck, ack.encode_to_vec()))
        };
        let nonce = challenge(&service, &mut conn);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, ack(&thief, nonce))),
            Status::Ok
        );
        let nonce = challenge(&service, &mut conn);
        assert_eq!(
            mailbox(&service, &mut conn, fetch(&device, nonce))
                .envelopes
                .len(),
            1
        );

        let nonce = challenge(&service, &mut conn);
        let request = ack(&device, nonce);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, request.clone())),
            Status::Ok
        );
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, request)),
            Status::Denied
        );
        let nonce = challenge(&service, &mut conn);
        assert!(mailbox(&service, &mut conn, fetch(&device, nonce))
            .envelopes
            .is_empty());

        // An ack forwarded by another replica needs no nonce of this connection, only the key.
        let response = mailbox(&service, &mut conn, put(envelope(vec![7; 16], vec![2])));
        assert_eq!(mailbox_status(&response), Status::Ok);
        let replica = |key: &Identity| {
            let ack = proto::Ack {
                nonce: vec![0; 32],
                ids: vec![vec![7; 16]],
            };
            mailbox_request::Request::ReplicaAck(key.sign(Domain::MailboxAck, ack.encode_to_vec()))
        };
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, replica(&thief))),
            Status::Ok
        );
        let nonce = challenge(&service, &mut conn);
        let left = mailbox(&service, &mut conn, fetch(&device, nonce));
        assert_eq!(left.envelopes.len(), 1);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, replica(&device))),
            Status::Ok
        );
        let nonce = challenge(&service, &mut conn);
        assert!(mailbox(&service, &mut conn, fetch(&device, nonce))
            .envelopes
            .is_empty());
    }

    #[test]
    fn full_stores_and_byte_rate_refuse_requests() {
        let identity = Identity::generate();
        let profile = || {
            publish(
                Profile {
                    version: 1,
                    ..Profile::default()
                }
                .sign(&identity),
            )
        };
        let envelope = proto::Envelope {
            id: vec![7; 16],
            mailbox: vec![1; 32],
            ciphertext: vec![1],
        };
        let put = mailbox_request::Request::Put(
            identity.sign(Domain::Envelope, envelope.encode_to_vec()),
        );

        let service = service_with(|l| (l.profiles_max_mb, l.messages_max_mb) = (0, 0));
        assert_eq!(
            status(&service.profile(&peer("p"), profile()).unwrap()),
            Status::Full
        );
        let response = mailbox(&service, &mut None, put.clone());
        assert_eq!(mailbox_status(&response), Status::Full);
        let service = service_with(|l| l.min_free_mb = u64::MAX);
        assert_eq!(
            status(&service.profile(&peer("p"), profile()).unwrap()),
            Status::Full
        );

        // The byte rate sheds profiles before the mailbox within a second.
        let service = service_with(|l| l.bytes_per_second = 1 << 30);
        service.traffic.add((1 << 30) / 100 * 92);
        let limited = status(&service.profile(&peer("p"), get(vec![0; 32])).unwrap());
        assert_eq!(limited, Status::RateLimited);
        let response = mailbox(&service, &mut None, put.clone());
        assert_eq!(
            mailbox_status(&response),
            Status::Ok,
            "mailbox shed before the limit"
        );
        service.traffic.add(1 << 30);
        let response = mailbox(&service, &mut None, put.clone());
        assert_eq!(mailbox_status(&response), Status::RateLimited);
        assert_eq!(service.refused[5].load(Ordering::Relaxed), 2);
        service.report();

        // Memory: refused from 80 % of max_memory_mb, without a strike; dropped from 100 %.
        let service = service_with(|l| l.strikes_to_ban = 1);
        service.memory.set(85);
        let response = mailbox(&service, &mut None, put.clone());
        assert_eq!(mailbox_status(&response), Status::RateLimited);
        assert!(!service.overloaded() && !service.reputation.banned("p"));
        service.memory.set(100);
        assert!(service.overloaded());
        service.memory.set(10);
        assert!(!service.overloaded());
        let response = mailbox(&service, &mut None, put);
        assert_eq!(mailbox_status(&response), Status::Ok);
    }

    #[test]
    fn group_and_sender_quotas_and_bans() {
        let service = service_with(|l| {
            (l.ip_group_requests_per_second, l.strikes_to_ban) = (2, 2);
            l.sender_puts_per_second = 1;
        });
        let id = || get(vec![0; 32]);
        // Two peer IDs in one IP group share its quota; another group is unaffected.
        assert_eq!(
            status(&service.profile(&peer("a"), id()).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&service.profile(&peer("b"), id()).unwrap()),
            Status::NotFound
        );
        let limited = service.profile(&peer("c"), id()).unwrap();
        assert_eq!(status(&limited), Status::RateLimited);
        let other = Peer {
            id: "d".into(),
            group: "h".into(),
        };
        assert_eq!(
            status(&service.profile(&other, id()).unwrap()),
            Status::NotFound
        );

        let sender = Identity::generate();
        let put = |id: u8| {
            let envelope = proto::Envelope {
                id: vec![id; 16],
                mailbox: vec![1; 32],
                ciphertext: vec![1],
            };
            mailbox_request::Request::Put(sender.sign(Domain::Envelope, envelope.encode_to_vec()))
        };
        let service = service_with(|l| {
            l.sender_puts_per_second = 1;
            l.strikes_to_ban = 2;
        });
        let sender_peer = Peer {
            id: "e".into(),
            group: "i".into(),
        };
        let put = |id| {
            let request = MailboxRequest {
                request: Some(put(id)),
            };
            mailbox_status(&service.mailbox(&sender_peer, &mut None, request).unwrap())
        };
        assert_eq!((put(1), put(2)), (Status::Ok, Status::RateLimited));

        // A second forged record bans the peer: even a valid request is refused.
        let mut forged = Profile::default().sign(&sender);
        forged.payload.push(0);
        let bad = |group: &str| Peer {
            id: "f".into(),
            group: group.into(),
        };
        for group in ["j", "k"] {
            let response = service.profile(&bad(group), publish(forged.clone()));
            assert_eq!(status(&response.unwrap()), Status::Denied);
        }
        assert!(service.reputation.banned("f"));
        let refused = service.profile(&bad("l"), id()).unwrap();
        assert_eq!(status(&refused), Status::RateLimited);
    }

    #[test]
    fn rate_limit_and_info() {
        let mut service = service();
        service.rate_limiter = PeerRateLimiter::new(1);
        let id = vec![0; 32];
        assert_eq!(
            status(&service.profile(&peer("p"), get(id.clone())).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&service.profile(&peer("p"), get(id.clone())).unwrap()),
            Status::RateLimited
        );
        assert_eq!(
            status(&service.profile(&peer("q"), get(id)).unwrap()),
            Status::NotFound
        );

        let info = service.node(
            &peer("q"),
            NodeRequest {
                request: Some(node_request::Request::Info(proto::InfoRequest {})),
            },
        );
        assert_eq!(
            info.info.unwrap().roles,
            vec![i32::from(proto::Role::Store)]
        );
        let deny_list = || NodeRequest {
            request: Some(node_request::Request::DenyList(proto::DenyListRequest {})),
        };
        let not_shared = service.node(&peer("q"), deny_list());
        assert_eq!(not_shared.status, i32::from(Status::NotFound));
        service.shared_deny = Some(SignedRecord::default());
        assert!(service.node(&peer("q"), deny_list()).deny_list.is_some());
        let turn = NodeRequest {
            request: Some(node_request::Request::Turn(proto::TurnRequest {})),
        };
        let unsupported = service.node(&peer("q"), turn);
        assert_eq!(unsupported.status, i32::from(Status::Unsupported));
        service.config.roles.push(Role::Turn);
        service.config.turn.secret = "secret".into();
        let credentials = service.node(&peer("t"), turn).turn.unwrap();
        assert!(credentials.username.ends_with(":t"));
        // Vector from Python: base64(hmac.new(b"secret", user, sha1).digest()).
        let known = turn_credentials(&service.config.turn, "12D3KooWpeer", 1_000_000);
        assert_eq!(known.username, "1003600:12D3KooWpeer");
        assert_eq!(known.password, "33Vkitp/KSZTCIYERO5wm1HgMoc="); // gitleaks:allow (test vector)
    }

    fn media_service(change: impl FnOnce(&mut crate::config::Limits)) -> Service {
        let mut s = service_with(change);
        s.media = Some(MediaStore::open(&s.config.storage.dir).unwrap());
        s
    }

    fn media_replica(owner: &Identity, data: Vec<u8>, version: u64) -> proto::MediaReplicaPut {
        let keep = proto::MediaKeep {
            version,
            hashes: vec![dyapp_identity::sha256(&data).to_vec()],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        };
        proto::MediaReplicaPut {
            data,
            manifest: None,
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
            )),
        }
    }

    #[test]
    fn media_size_hints_preserve_object_boundaries() {
        use proto::media_replica_put::Authorization;
        let s = media_service(|l| {
            l.media_requests_per_second = 100;
            l.bytes_per_second = 0;
        });
        let owner = Identity::generate();
        let max = crate::replication::MEDIA_OBJECT_BYTES;
        let call = |request| {
            s.media(
                &peer("media-size"),
                MediaRequest {
                    request: Some(request),
                },
            )
            .unwrap()
        };
        let put = |size| {
            media_request::Request::Put(proto::MediaPut {
                owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
                data: vec![0; size],
            })
        };
        let exact = call(put(MAX_MEDIA_BYTES));
        assert_eq!(
            (exact.status, exact.max_bytes),
            (i32::from(Status::NotFound), 0)
        );
        let over = call(put(MAX_MEDIA_BYTES + 1));
        assert_eq!(
            (over.status, over.max_bytes),
            (i32::from(Status::TooLarge), MAX_MEDIA_BYTES as u64)
        );
        let exact = media_replica(&owner, vec![7; max], 1);
        assert_eq!(s.store_media(&exact).unwrap().status, i32::from(Status::Ok));
        let over = media_replica(&owner, vec![7; max + 1], 2);
        let response = s.store_media(&over).unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::TooLarge), max as u64)
        );
        let data = b"authorization-boundary".to_vec();
        let hash = dyapp_identity::sha256(&data).to_vec();
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let keep = proto::MediaKeep {
            version: 2,
            hashes: vec![hash.clone()],
            time: now,
        };
        let attach = proto::MediaAttach {
            release: vec![9; 32],
            hashes: vec![hash],
            created: now,
        };
        for (domain, payload) in [
            (Domain::MediaKeep, keep.encode_to_vec()),
            (Domain::MediaAttach, attach.encode_to_vec()),
        ] {
            for size in [max, max + 1] {
                // For these sizes both payload varints have the same length.
                let probe = padded_record(&owner, domain, payload.clone(), max);
                let overhead = probe.encoded_len() - probe.payload.len();
                let record = padded_record(&owner, domain, payload.clone(), size - overhead);
                assert_eq!(record.encoded_len(), size);
                let (request, authorization) = if domain == Domain::MediaKeep {
                    (
                        media_request::Request::Keep(record.clone()),
                        Authorization::Keep(record),
                    )
                } else {
                    (
                        media_request::Request::Attach(record.clone()),
                        Authorization::Attach(record),
                    )
                };
                let direct = call(request);
                let replica = proto::MediaReplicaPut {
                    data: data.clone(),
                    authorization: Some(authorization),
                    manifest: None,
                };
                let stored = s.store_media(&replica).unwrap();
                for response in [direct, stored] {
                    assert_eq!(
                        (response.status, response.max_bytes),
                        if size == max {
                            (i32::from(Status::Ok), 0)
                        } else {
                            (i32::from(Status::TooLarge), max as u64)
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn media_authorization_records_are_bounded_before_storage() {
        use proto::media_replica_put::Authorization;
        let s = media_service(|_| {});
        let peer = peer("media-proof-limit");
        let record = SignedRecord {
            payload: vec![0; crate::replication::MEDIA_OBJECT_BYTES],
            ..Default::default()
        };
        for request in [
            media_request::Request::Keep(record.clone()),
            media_request::Request::Attach(record.clone()),
        ] {
            assert_eq!(
                s.media(
                    &peer,
                    MediaRequest {
                        request: Some(request)
                    }
                )
                .unwrap()
                .status,
                Status::TooLarge as i32,
            );
        }
        for authorization in [
            Authorization::Keep(record.clone()),
            Authorization::Attach(record),
        ] {
            let replica = proto::MediaReplicaPut {
                authorization: Some(authorization),
                ..Default::default()
            };
            assert_eq!(
                s.store_media(&replica).unwrap().status,
                Status::TooLarge as i32
            );
        }
    }

    #[test]
    fn media_replicas_verify_proofs_and_never_restore_dropped_hashes() {
        use proto::media_replica_put::Authorization;
        let mut s = media_service(|_| {});
        let owner = Identity::generate();
        let replica = media_replica(&owner, b"replica".to_vec(), 1);
        let hash = dyapp_identity::sha256(&replica.data);
        let result = |s: &Service, replica: &proto::MediaReplicaPut| {
            Status::try_from(s.store_media(replica).unwrap().status).unwrap()
        };
        let mut bad = replica.clone();
        bad.authorization = None;
        assert_eq!(result(&s, &bad), Status::Denied);
        bad.authorization = Some(Authorization::Keep(SignedRecord::default()));
        assert_eq!(result(&s, &bad), Status::Denied);
        bad.data.push(1);
        bad.authorization = replica.authorization.clone();
        assert_eq!(result(&s, &bad), Status::NotFound);
        assert_eq!(result(&s, &replica), Status::Ok);
        assert_eq!(
            result(&s, &replica),
            Status::Ok,
            "already held counts as stored"
        );
        // The signed proof survives a restart and supports acceptor forwarding.
        s.media = Some(MediaStore::open(&s.config.storage.dir).unwrap());
        assert_eq!(
            s.media
                .as_ref()
                .unwrap()
                .authorization(&owner.peer_id(), &hash)
                .unwrap(),
            replica.authorization,
        );
        let newer = proto::MediaKeep {
            version: 2,
            hashes: Vec::new(),
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        };
        let record = owner.sign(Domain::MediaKeep, newer.encode_to_vec());
        s.media
            .as_ref()
            .unwrap()
            .keep_signed(&owner.peer_id(), &newer, &record)
            .unwrap();
        assert_eq!(result(&s, &replica), Status::NotFound);
        assert_eq!(s.media.as_ref().unwrap().get(&hash).unwrap(), None);
        assert_eq!(
            s.media
                .as_ref()
                .unwrap()
                .authorization(&owner.peer_id(), &hash)
                .unwrap(),
            None
        );
        // Even if the latest keep permits a hash, a carried proof must name it too.
        let current = media_replica(&owner, replica.data.clone(), 3);
        assert_eq!(result(&s, &current), Status::Ok);
        bad = current;
        bad.authorization = Some(Authorization::Keep(record));
        assert_eq!(result(&s, &bad), Status::NotFound);
        let attach = proto::MediaAttach {
            release: dyapp_identity::sha256(b"release").to_vec(),
            hashes: vec![hash.to_vec()],
            created: chrono::Utc::now().timestamp().unsigned_abs(),
        };
        let mut attachment = replica.clone();
        attachment.authorization = Some(Authorization::Attach(
            owner.sign(Domain::MediaAttach, attach.encode_to_vec()),
        ));
        assert_eq!(result(&s, &attachment), Status::Ok);
        s.media = Some(MediaStore::open(&s.config.storage.dir).unwrap());
        assert_eq!(
            s.media
                .as_ref()
                .unwrap()
                .authorization(&owner.peer_id(), &hash)
                .unwrap(),
            attachment.authorization,
            "unexpired attachments take precedence over potentially aged keeps"
        );
        // Releasing the attachment leaves the independently kept copy alive.
        assert!(s.media.as_ref().unwrap().release(b"release").unwrap());
        assert_eq!(result(&s, &replica), Status::Ok);
        let expired = proto::MediaAttach {
            created: 1,
            ..attach
        };
        attachment.authorization = Some(Authorization::Attach(
            owner.sign(Domain::MediaAttach, expired.encode_to_vec()),
        ));
        assert_eq!(result(&s, &attachment), Status::Invalid);
    }

    #[test]
    fn media_bills_whole_copy_puts_five_times_and_gets_by_reply_bytes() {
        let mut s = media_service(|l| l.bytes_per_second = 10_000);
        let owner = Identity::generate();
        let replica = media_replica(&owner, vec![1; 1000], 1);
        assert_eq!(
            s.store_media(&replica).unwrap().status,
            i32::from(Status::Ok)
        );
        let put = proto::MediaPut {
            owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
            data: replica.data.clone(),
        };
        assert!(s.prepare_media(&peer("client"), &put).unwrap().is_ok());
        assert_eq!(
            s.traffic.second(),
            50,
            "intake reserves 5 x 1000 bytes, plus framing"
        );
        let get = MediaRequest {
            request: Some(media_request::Request::Get(proto::GetMedia {
                hash: dyapp_identity::sha256(&put.data).to_vec(),
            })),
        };
        assert_eq!(s.media(&peer("client"), get).unwrap().data, put.data);
        assert_eq!(
            s.traffic.second(),
            60,
            "get reserves exactly the reply payload"
        );
        // Isolate peer billing from the shared traffic guard.
        s.traffic = Traffic::new(0);
        let big = media_replica(&owner, vec![2; 1600], 2);
        assert_eq!(s.store_media(&big).unwrap().status, i32::from(Status::Ok));
        let put = proto::MediaPut {
            data: big.data.clone(),
            ..put
        };
        assert!(s
            .prepare_media(&replica_peer("gallery"), &put)
            .unwrap()
            .is_ok());
        assert_eq!(
            s.prepare_media(&replica_peer("gallery"), &put).unwrap(),
            Err(Status::RateLimited)
        );
        let node_put = || MediaRequest {
            request: Some(media_request::Request::ReplicaPut(big.clone())),
        };
        assert_eq!(
            s.media(&replica_peer("gallery"), node_put())
                .unwrap()
                .status,
            i32::from(Status::Ok)
        );
        assert_eq!(
            s.media(&replica_peer("gallery"), node_put())
                .unwrap()
                .status,
            i32::from(Status::RateLimited)
        );
        assert_eq!(
            s.media(&replica_peer("other-node"), node_put())
                .unwrap()
                .status,
            i32::from(Status::Ok)
        );
    }

    #[test]
    fn media_whole_copy_owner_quota_includes_five_copies() {
        let s = media_service(|l| l.media_per_owner_mb = 1);
        let owner = Identity::generate();
        let replica = media_replica(&owner, vec![1; 210_000], 1);
        assert_eq!(
            s.store_media(&replica).unwrap().status,
            i32::from(Status::Full)
        );
        let replica = media_replica(&owner, vec![2; 200_000], 2);
        assert_eq!(
            s.store_media(&replica).unwrap().status,
            i32::from(Status::Ok)
        );
        // A second owner cannot bypass its quota by listing a copy already on this node.
        let other = Identity::generate();
        let first = media_replica(&other, vec![3; 20_000], 1);
        assert_eq!(s.store_media(&first).unwrap().status, i32::from(Status::Ok));
        let shared = media_replica(&other, replica.data.clone(), 2);
        let keep = proto::MediaKeep {
            version: 2,
            hashes: vec![
                dyapp_identity::sha256(&first.data).to_vec(),
                dyapp_identity::sha256(&shared.data).to_vec(),
            ],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        };
        let shared = proto::MediaReplicaPut {
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                other.sign(Domain::MediaKeep, keep.encode_to_vec()),
            )),
            ..shared
        };
        assert_eq!(
            s.store_media(&shared).unwrap().status,
            i32::from(Status::Full)
        );
    }

    #[test]
    fn media_manifests_shards_quota_and_corruption() {
        let s = media_service(|_| {});
        let owner = Identity::generate();
        let data = vec![7; (1 << 20) + 1];
        let mut replica = media_replica(&owner, data.clone(), 1);
        let (manifest, shards) = crate::replication::media_encode(&data).unwrap();
        replica.data.clear();
        replica.manifest = Some(manifest.clone());
        assert_eq!(
            s.store_media(&replica).unwrap().status,
            i32::from(Status::Ok)
        );
        assert_eq!(
            s.media.as_ref().unwrap().manifest(&manifest.hash).unwrap(),
            Some(manifest.clone())
        );
        let mut conflicting = replica.clone();
        conflicting.manifest.as_mut().unwrap().shard_hashes[0][0] ^= 1;
        assert_eq!(
            s.store_media(&conflicting).unwrap().status,
            i32::from(Status::Invalid)
        );
        let mut put = proto::MediaShardPut {
            replica: Some(replica.clone()),
            index: 5,
            data: shards[5].clone(),
        };
        put.data[0] ^= 1;
        assert_eq!(
            s.store_shard(&put).unwrap().status,
            i32::from(Status::Invalid)
        );
        put.data = shards[5].clone();
        assert_eq!(s.store_shard(&put).unwrap().status, i32::from(Status::Ok));
        let fragments = vec![(4, shards[4].clone()), (5, shards[5].clone())];
        assert_eq!(s.assemble_media(&manifest, &fragments).unwrap().data, data);
        let persisted = MediaStore::open(&s.config.storage.dir).unwrap();
        assert_eq!(
            persisted.shard(&manifest.hash, 5).unwrap(),
            Some(shards[5].clone())
        );
        assert!(persisted.usage().unwrap().0 <= (1 << 20) + 1024);
        let mut wrong = manifest.clone();
        wrong.hash[0] ^= 1;
        let keep = proto::MediaKeep {
            version: 2,
            hashes: vec![wrong.hash.clone()],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        };
        replica.manifest = Some(wrong.clone());
        replica.authorization = Some(proto::media_replica_put::Authorization::Keep(
            owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
        ));
        assert_eq!(
            s.store_media(&replica).unwrap().status,
            i32::from(Status::Ok)
        );
        put.replica = Some(replica);
        assert_eq!(s.store_shard(&put).unwrap().status, i32::from(Status::Ok));
        assert_eq!(
            s.assemble_media(&wrong, &fragments).unwrap().status,
            i32::from(Status::NotFound)
        );
        assert_eq!(persisted.manifest(&wrong.hash).unwrap(), None);
        assert_eq!(persisted.shard(&wrong.hash, 5).unwrap(), None);
        let quota = media_service(|l| l.media_per_owner_mb = 1);
        assert_eq!(
            quota
                .store_media(put.replica.as_ref().unwrap())
                .unwrap()
                .status,
            i32::from(Status::Full)
        );
    }

    #[test]
    fn a_verified_whole_copy_wins_and_sharded_uploads_pay_their_ratio() {
        let mut s = media_service(|l| l.bytes_per_second = 4 << 20);
        let owner = Identity::generate();
        let small = media_replica(&owner, b"whole".to_vec(), 1);
        assert_eq!(s.store_media(&small).unwrap().status, i32::from(Status::Ok));
        let (mut manifest, _) = crate::replication::media_encode(&small.data).unwrap();
        manifest.shard_hashes[0][0] ^= 1;
        let mut proof = small.clone();
        proof.data.clear();
        proof.manifest = Some(manifest.clone());
        assert_eq!(s.store_media(&proof).unwrap().status, i32::from(Status::Ok));
        assert_eq!(
            s.media.as_ref().unwrap().manifest(&manifest.hash).unwrap(),
            None
        );
        assert_eq!(s.assemble_media(&manifest, &[]).unwrap().data, small.data);
        // A manifest already held must not suppress a later verified whole-copy put.
        let second = media_service(|_| {});
        assert_eq!(
            second.store_media(&proof).unwrap().status,
            i32::from(Status::Ok)
        );
        assert!(second
            .media
            .as_ref()
            .unwrap()
            .manifest(&manifest.hash)
            .unwrap()
            .is_some());
        assert_eq!(
            second.store_media(&small).unwrap().status,
            i32::from(Status::Ok)
        );
        assert_eq!(
            second.media.as_ref().unwrap().get(&manifest.hash).unwrap(),
            Some(small.data.clone())
        );
        assert_eq!(
            second
                .media
                .as_ref()
                .unwrap()
                .manifest(&manifest.hash)
                .unwrap(),
            None
        );
        let data = vec![4; (1 << 20) + 1];
        let mut replica = media_replica(&owner, data.clone(), 2);
        let (manifest, _) = crate::replication::media_encode(&data).unwrap();
        assert_eq!(manifest.billed_bytes(), Some(3 * data.len() as u64));
        replica.data.clear();
        replica.manifest = Some(manifest);
        assert_eq!(
            s.store_media(&replica).unwrap().status,
            i32::from(Status::Ok)
        );
        s.traffic = Traffic::new(0);
        let put = proto::MediaPut {
            owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
            data,
        };
        assert!(s.prepare_media(&peer("uploader"), &put).unwrap().is_ok());
        assert_eq!(
            s.prepare_media(&peer("uploader"), &put).unwrap(),
            Err(Status::RateLimited)
        );
    }

    #[test]
    fn media_keep_put_get() {
        use media_request::Request::{Get, Keep, Put};
        // Each peer within the media request rate.
        let call_as = |service: &Service, name: &str, request| {
            let response = service.media(
                &peer(name),
                MediaRequest {
                    request: Some(request),
                },
            );
            let response = response.unwrap();
            (Status::try_from(response.status).unwrap(), response)
        };
        let call = |service: &Service, request| call_as(service, "p", request);
        let get = |hash: &[u8]| {
            Get(proto::GetMedia {
                hash: hash.to_vec(),
            })
        };
        let mut service = service();
        assert_eq!(call(&service, get(&[0; 32])).0, Status::Unsupported);

        let dir = format!("/tmp/ai/test-service-media-{}", uuid::Uuid::new_v4());
        service.media = Some(MediaStore::open(std::path::Path::new(&dir)).unwrap());
        let identity = Identity::generate();
        let owner = dyapp_identity::key_hash(&identity.public_key()).to_vec();
        let data = b"photo".to_vec();
        let hash = dyapp_identity::sha256(&data).to_vec();
        let put = || {
            Put(proto::MediaPut {
                owner: owner.clone(),
                data: data.clone(),
            })
        };
        let keep = proto::MediaKeep {
            version: 1,
            hashes: vec![hash.clone()],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        };
        for time in [keep.time - 3600, keep.time + 3600] {
            let off_clock = proto::MediaKeep {
                time,
                ..keep.clone()
            };
            let signed = identity.sign(Domain::MediaKeep, off_clock.encode_to_vec());
            assert_eq!(call_as(&service, "clock", Keep(signed)).0, Status::Invalid);
        }
        let keep = identity.sign(Domain::MediaKeep, keep.encode_to_vec());

        assert_eq!(call(&service, put()).0, Status::NotFound);
        let (got, response) = call(&service, Keep(keep.clone()));
        assert_eq!((got, response.missing), (Status::Ok, vec![hash.clone()]));
        assert_eq!(call(&service, Keep(keep.clone())).0, Status::Stale);
        assert_eq!(call(&service, put()).0, Status::Ok);
        let (got, response) = call(&service, get(&hash));
        assert_eq!((got, response.data), (Status::Ok, data));
        assert_eq!(call(&service, get(&[1; 5])).0, Status::Invalid);

        let mut forged = keep;
        forged.payload.push(0);
        assert_eq!(call(&service, Keep(forged)).0, Status::Denied);

        // An attachment: put after the attach, gone after the release.
        use media_request::Request::{Attach, Release};
        let file = b"attachment".to_vec();
        let file_hash = dyapp_identity::sha256(&file).to_vec();
        let secret = vec![7u8; 32];
        let attach = |created| {
            let attach = proto::MediaAttach {
                release: dyapp_identity::sha256(&secret).to_vec(),
                hashes: vec![file_hash.clone()],
                created,
            };
            Attach(identity.sign(Domain::MediaAttach, attach.encode_to_vec()))
        };
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        assert_eq!(
            call_as(&service, "q", attach(now + 3600)).0,
            Status::Invalid
        );
        let (got, response) = call_as(&service, "q", attach(now));
        assert_eq!(
            (got, response.missing),
            (Status::Ok, vec![file_hash.clone()])
        );
        let put = Put(proto::MediaPut {
            owner: owner.clone(),
            data: file,
        });
        assert_eq!(call_as(&service, "q", put).0, Status::Ok);
        assert_eq!(call_as(&service, "q", get(&file_hash)).0, Status::Ok);
        let release = |secret: &[u8]| {
            Release(proto::MediaRelease {
                secret: secret.to_vec(),
            })
        };
        assert_eq!(
            call_as(&service, "q", release(&[8; 32])).0,
            Status::NotFound
        );
        assert_eq!(call_as(&service, "q", release(&secret)).0, Status::Ok);
        assert_eq!(call_as(&service, "q", get(&file_hash)).0, Status::NotFound);
        assert_eq!(
            call_as(&service, "q", get(&hash)).0,
            Status::Ok,
            "kept blob stays"
        );
        service.deny = [hex(&hash)].into();
        assert_eq!(
            call_as(&service, "r", get(&hash)).0,
            Status::Refused,
            "blocked blob"
        );

        let info = service.node(
            &peer("q"),
            NodeRequest {
                request: Some(node_request::Request::Info(proto::InfoRequest {})),
            },
        );
        let info = info.info.unwrap();
        assert_eq!(info.roles.len(), 2);
        assert_eq!(info.max_media_bytes, MAX_MEDIA_BYTES as u64);
    }
}
