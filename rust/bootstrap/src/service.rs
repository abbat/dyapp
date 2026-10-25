//! Answers node protocol requests (`/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`,
//! `/dyapp/media`) from the stores. The libp2p loop in `dyappd` passes each request here and sends back the reply.

#[path = "service/keypackages.rs"]
mod keypackages;

use crate::replication::MediaManifestExt;
use crate::{
    config::{Role, Turn},
    maintenance::RebuildBudget,
    media::{Attached, MediaStore, Put},
    pow::PowCheck,
    rate_limit::{Memory, PeerRateLimiter, Reputation, Traffic},
    search::{SearchIndex, SEARCH_FIELDS},
    BootstrapError, BootstrapStore, NodeConfig,
};
use dyapp_identity::{Domain, SignedRecord};
use dyapp_p2p_net::proto::{
    self, mailbox_request, media_request, node_request, profile_request, search_request,
    DigestRequest, DigestResponse, InventoryRequest, InventoryResponse, MailboxRequest,
    MailboxResponse, MediaRequest, MediaResponse, NodeInfo, NodeRequest, NodeResponse,
    ProfileRequest, ProfileResponse, SearchRequest, SearchResponse, Status, TurnCredentials,
};
use prost::Message;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

/// Envelope payload send limit advertised in node info.
pub const MAX_ENVELOPE_BYTES: usize = 100 * 1024;
/// Network acceptance is raised before the send limit during a rollout.
const ENVELOPE_ACCEPT_BYTES: usize = MAX_ENVELOPE_BYTES;
const PROFILE_SEND_BYTES: usize = dyapp_profile::MAX_PAYLOAD_LEN;
const PROFILE_ACCEPT_BYTES: usize = dyapp_profile::MAX_PAYLOAD_LEN;
/// Fixed network record admission; raise before increasing the client-send default.
pub(crate) const SEARCH_ACCEPT_BYTES: usize = 64 << 10;
/// Largest signed payload of a SIGNAL envelope, sent and accepted.
pub const MAX_SIGNAL_BYTES: usize = 1024;
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
    /// libp2p peer ID.
    pub id: String,
    /// IP group of the remote address.
    pub group: String,
    /// The wider group of the same address, by `coarse_ipv4_prefix` / `coarse_ipv6_prefix`.
    pub coarse: String,
}

/// Request handling of every role, shared by the swarm loop and the blocking pool.
pub struct Service {
    /// Profile and mailbox stores.
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
    /// The hash puzzle clients' queries and gets need while the search queue is slow.
    pub puzzle: crate::puzzle::Puzzle,
    /// Requests per second of one peer.
    pub rate_limiter: PeerRateLimiter,
    /// Media requests per second of one peer.
    pub media_peers: PeerRateLimiter,
    media_bytes: PeerRateLimiter,
    /// Requests per second of one IP group.
    pub groups: PeerRateLimiter,
    /// Requests per second of one coarse IP group, at ten times the group rate.
    coarse_groups: PeerRateLimiter,
    /// Envelope puts per second of one sender key.
    pub senders: PeerRateLimiter,
    /// Strikes and bans.
    pub reputation: Reputation,
    /// Node protocol bytes per second.
    pub traffic: Traffic,
    /// Successful protobuf frame bytes, shared with swarm codecs.
    pub wire: Arc<dyapp_p2p_net::WireBytes>,
    /// The swarm's outgoing deny filter, for its refusal count.
    pub dial_filter: dyapp_p2p_net::DialFilter,
    /// Network IDs shared with this node's swarm; configuration still selects public.
    pub protocols: Arc<dyapp_p2p_net::Protocols>,
    /// Process memory against `limits.max_memory_mb`.
    pub memory: Memory,
    /// Write budget of background rebuilds.
    pub rebuild_budget: RebuildBudget,
    /// The node configuration.
    pub config: NodeConfig,
    /// The operator's deny list: libp2p peer IDs, IP groups and hex key hashes.
    /// Locked because the node reloads it while store workers read it.
    pub deny: RwLock<HashSet<String>>,
    /// Signs the shared deny list, with `network.share_deny_list` on.
    pub deny_signer: Option<dyapp_identity::Identity>,
    /// The deny list as it is shared, signed when it was last loaded.
    pub shared_deny: RwLock<Option<SignedRecord>>,
    /// Guards (in `GUARDS` order) that refused the last request they checked. A change is
    /// logged once, not per request.
    tripped: [AtomicBool; 5],
    /// Requests each guard refused since start; the last two count the byte rate and requests
    /// on a connection whose IP is unknown, neither logged when it trips.
    refused: [AtomicU64; 7],
    /// `refused` at the last `report`.
    reported: [AtomicU64; 7],
    /// Request, connection and storage metrics.
    pub metrics: crate::metrics::Metrics,
    /// Keys envelope source groups; in memory only, so a seized disk cannot map them back to IP
    /// groups, and new on restart.
    salt: [u8; 32],
}

const GUARDS: [&str; 7] = [
    "disk: profiles",
    "disk: messages",
    "disk: media",
    "memory: rate limited",
    "memory: dropped",
    "traffic: bytes per second",
    "endpoint: unknown",
];

impl Service {
    /// A service over `store` with every limit from `config`, no other role store and no deny list.
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
            puzzle: crate::puzzle::Puzzle::new(
                config.search.puzzle_wait_ms,
                config.search.max_puzzle_bits,
                std::time::Instant::now(),
            ),
            rate_limiter: PeerRateLimiter::new(l.requests_per_second),
            media_peers: PeerRateLimiter::new(l.media_requests_per_second),
            media_bytes: PeerRateLimiter::new(
                u32::try_from(l.bytes_per_second).unwrap_or(u32::MAX).max(1),
            ),
            groups: PeerRateLimiter::new(l.ip_group_requests_per_second),
            // Saturates for an unvalidated config, such as the fuzz node's unlimited one.
            coarse_groups: PeerRateLimiter::new(l.ip_group_requests_per_second.saturating_mul(10)),
            senders: PeerRateLimiter::new(l.sender_puts_per_second),
            reputation: Reputation::new(l.strikes_to_ban, ban),
            traffic: Traffic::new(l.bytes_per_second),
            wire: Arc::default(),
            dial_filter: dyapp_p2p_net::DialFilter::default(),
            protocols: Arc::new(dyapp_p2p_net::Protocols::public()),
            memory: Memory::new(l.max_memory_mb.saturating_mul(1 << 20)),
            rebuild_budget: RebuildBudget::new(config.maintenance.io_mb_per_s),
            config,
            deny: RwLock::default(),
            deny_signer: None,
            shared_deny: RwLock::default(),
            tripped: Default::default(),
            refused: Default::default(),
            reported: Default::default(),
            metrics: crate::metrics::Metrics::default(),
            salt: crate::random(),
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

    /// Requests each guard refused since start, in `GUARDS` order.
    pub fn refusals(&self) -> [u64; 7] {
        std::array::from_fn(|i| self.refused[i].load(Ordering::Relaxed))
    }

    /// Runs the stores' and the search index's maintenance, freeing at most `pages` pages
    /// each, and logs the free pages left; called hourly.
    pub fn maintain(&self, pages: u32) {
        match self.store.maintain(pages) {
            Ok([profiles, messages]) => {
                tracing::info!(
                    profiles,
                    messages,
                    "store maintenance done, free pages left"
                )
            }
            Err(error) => tracing::error!(%error, "store maintenance failed"),
        }
        if let Some(index) = &self.search {
            match index.maintain(pages) {
                Ok(index) => {
                    tracing::info!(index, "search index maintenance done, free pages left")
                }
                Err(error) => tracing::error!(%error, "search index maintenance failed"),
            }
        }
    }

    /// Logs the refusals per guard since the last call; called hourly.
    pub fn report(&self) {
        let totals = self.refusals();
        for ((name, total), reported) in GUARDS.iter().zip(totals).zip(&self.reported) {
            let refused = total.saturating_sub(reported.fetch_max(total, Ordering::Relaxed));
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
        // An unknown IP is the node's gap, not the peer's fault.
        if !admitted && !peer.group.is_empty() {
            self.strike(peer);
        }
        admitted
    }

    /// Replication can exceed shared buckets through no fault of the sending node.
    fn admit_quiet(&self, peer: &Peer, units: u32) -> bool {
        !self.reputation.banned(&peer.id)
            && self.rate_limiter.check_units(&peer.id, units)
            && self.group_admits(peer, units)
    }

    /// Spends `units` from the IP group's and the coarse group's buckets. A connection whose IP
    /// is not known yet has empty groups: it is refused and counted, never pooled with other
    /// such connections.
    fn group_admits(&self, peer: &Peer, units: u32) -> bool {
        if peer.group.is_empty() || peer.coarse.is_empty() {
            self.refused[6].fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.groups.check_units(&peer.group, units)
            && self.coarse_groups.check_units(&peer.coarse, units)
    }

    fn node_peer(&self, peer: &Peer) -> bool {
        peer.id
            .parse()
            .is_ok_and(|id| dyapp_p2p_net::id_has_pow(&id, self.config.network.id_pow_bits))
    }

    /// Whether the deny list names `peer` or its IP group. A refusal is no strike: a listed
    /// user is the operator's choice, not misbehaviour.
    fn refused(&self, peer: &Peer) -> bool {
        let deny = self.deny.read().unwrap_or_else(PoisonError::into_inner);
        deny.contains(&peer.id) || deny.contains(&peer.group)
    }

    /// Whether the deny list names `address` (an identity's peer ID, a mailbox address or a
    /// media blob hash).
    pub(crate) fn listed(&self, address: &[u8]) -> bool {
        self.deny
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&hex(address))
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

    /// Handles a node control request from `peer`.
    pub fn node(&self, peer: &Peer, request: NodeRequest) -> NodeResponse {
        self.traffic.add(request.encoded_len() as u64);
        match request.request {
            Some(node_request::Request::Turn(_)) => self.turn(peer),
            // Info stays cheap for discovery: only the IP-group bucket, no per-peer strikes.
            Some(node_request::Request::Info(_)) if self.refused(peer) => {
                node_status(Status::Refused)
            }
            Some(node_request::Request::Info(_)) if !self.group_admits(peer, 1) => {
                node_status(Status::RateLimited)
            }
            Some(node_request::Request::Info(_)) => NodeResponse {
                status: Status::Ok.into(),
                info: Some(self.info()),
                ..NodeResponse::default()
            },
            Some(node_request::Request::DenyList(_)) if !self.admit(peer) => {
                node_status(Status::RateLimited)
            }
            Some(node_request::Request::DenyList(_)) if self.refused(peer) => {
                node_status(Status::Refused)
            }
            Some(node_request::Request::DenyList(_)) => {
                let shared = self
                    .shared_deny
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                NodeResponse {
                    status: match shared {
                        Some(_) => Status::Ok,
                        None => Status::NotFound,
                    }
                    .into(),
                    deny_list: shared,
                    ..NodeResponse::default()
                }
            }
            None => node_status(Status::Unsupported),
        }
    }

    /// Credentials on the coturn relay next to this node, bound to the asking peer's IP group so
    /// that coturn's `user-quota` counts per group, not per free peer ID.
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
            turn: Some(turn_credentials(
                &self.config.turn,
                &turn_user(&self.config.turn, &peer.group),
                now,
            )),
            ..node_status(Status::Ok)
        }
    }

    fn info(&self) -> NodeInfo {
        let store = self.config.roles.contains(&Role::Store);
        let media = self.media.is_some();
        let turn = self.config.roles.contains(&Role::Turn);
        let search = self.search.is_some();
        let puzzle = if search {
            self.puzzle.current()
        } else {
            ([0; 16], 0)
        };
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
            max_signal_bytes: if store { MAX_SIGNAL_BYTES as u64 } else { 0 },
            signal_quota_bytes: if store { self.signal_quota() } else { 0 },
            mailbox_pool_bytes: if store { self.mailbox_pool() } else { 0 },
            require_write_tokens: store && self.config.limits.require_write_tokens,
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
            min_profile_pow: if store {
                self.config.search.min_profile_pow_bits
            } else {
                0
            },
            min_heartbeat_pow: if search {
                self.config.search.min_heartbeat_pow_bits
            } else {
                0
            },
            puzzle_bits: puzzle.1,
            puzzle_challenge: if search { puzzle.0.to_vec() } else { vec![] },
            server_time: chrono::Utc::now().timestamp().unsigned_abs(),
            capabilities: [
                (store, proto::Capability::ProfileHeartbeat),
                (search, proto::Capability::SearchPuzzle),
                (
                    self.config.network.share_deny_list,
                    proto::Capability::SharedDenyList,
                ),
            ]
            .into_iter()
            .filter_map(|(on, capability)| on.then_some(capability.into()))
            .collect(),
        }
    }

    fn retention(&self) -> u64 {
        u64::from(self.config.limits.message_ttl_hours) * 3600
    }

    /// Signal bytes kept per mailbox.
    fn signal_quota(&self) -> u64 {
        self.config.limits.signal_max_kb.saturating_mul(1024)
    }

    /// Tokenless message bytes kept per mailbox while write tokens are required.
    fn mailbox_pool(&self) -> u64 {
        self.config.limits.mailbox_pool_kb.saturating_mul(1024)
    }

    /// `peer` is the key for rate limiting and strikes. A storage failure is an
    /// error: the caller drops the request and the client tries another node.
    pub fn profile(&self, peer: &Peer, request: ProfileRequest) -> crate::Result<ProfileResponse> {
        self.profile_inner(peer, request, true, &|_| None)
    }

    /// Handles a network request in a blocking store worker. The callback may run Argon2
    /// after admission, but must never be passed from the swarm loop.
    pub(crate) fn profile_with_pow(
        &self,
        peer: &Peer,
        request: ProfileRequest,
        check_pow: &impl Fn(&PowCheck) -> Option<bool>,
    ) -> crate::Result<ProfileResponse> {
        self.profile_inner(peer, request, true, check_pow)
    }

    /// Admits and checks a profile publish from `peer` without storing it.
    pub fn prepare_profile(
        &self,
        peer: &Peer,
        record: SignedRecord,
    ) -> crate::Result<ProfileResponse> {
        self.prepare_profile_with_pow(peer, record, &|_| None)
    }

    /// Checks a client publish in a blocking store worker before replica fan-out.
    pub(crate) fn prepare_profile_with_pow(
        &self,
        peer: &Peer,
        record: SignedRecord,
        check_pow: &impl Fn(&PowCheck) -> Option<bool>,
    ) -> crate::Result<ProfileResponse> {
        self.profile_inner(
            peer,
            ProfileRequest {
                request: Some(profile_request::Request::Publish(record)),
                age_seconds: 0,
            },
            false,
            check_pow,
        )
    }

    fn profile_inner(
        &self,
        peer: &Peer,
        request: ProfileRequest,
        store: bool,
        check_pow: &impl Fn(&PowCheck) -> Option<bool>,
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
                let mut last_seen = None;
                if is_replica && request.age_seconds > 0 {
                    let ttl = u64::from(self.config.limits.profile_ttl_days) * 86_400;
                    if u64::from(request.age_seconds) >= ttl {
                        // The owner would already have expired here: accept without storing.
                        return Ok(reply(Status::Ok, None));
                    }
                    let now = chrono::Utc::now().timestamp();
                    last_seen = Some(now - i64::from(request.age_seconds));
                }
                let response = self.publish(&record, store, max, last_seen, check_pow)?;
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

    /// Verifies and stores a profile record, without per-peer limits.
    pub fn store_profile(&self, record: &SignedRecord) -> crate::Result<ProfileResponse> {
        self.publish(record, true, PROFILE_ACCEPT_BYTES, None, &|_| None)
    }

    fn publish(
        &self,
        record: &SignedRecord,
        store: bool,
        max: usize,
        last_seen: Option<i64>,
        check_pow: &impl Fn(&PowCheck) -> Option<bool>,
    ) -> crate::Result<ProfileResponse> {
        if record.payload.len() > max {
            return Ok(profile_too_large(max));
        }
        if self.listed_key(&record.public_key) {
            return Ok(reply(Status::Refused, None));
        }
        let verified = match dyapp_profile::verify(record) {
            Ok(verified) => verified,
            Err(dyapp_profile::Error::Signature(_)) => return Ok(reply(Status::Denied, None)),
            Err(_) => return Ok(reply(Status::Invalid, None)),
        };
        let profile = &verified.profile;
        let minimum = self.config.search.min_profile_pow_bits;
        if profile.pow_bits < minimum {
            return Ok(ProfileResponse {
                required_pow_bits: minimum,
                ..reply(Status::Invalid, None)
            });
        }
        // Zero difficulty needs no hash. Every positive claim must be checked, even on a
        // zero-minimum test network, or a forged claim could enter a store.
        if profile.pow_bits > 0 {
            let check = PowCheck {
                key: record.public_key.as_slice().try_into().unwrap_or_default(),
                nonce: profile.pow_nonce,
                bits: profile.pow_bits,
                day: None,
            };
            if !self.pow.confirmed(&check) {
                match check_pow(&check) {
                    Some(true) => {}
                    Some(false) => return Ok(reply(Status::Denied, None)),
                    None => return Ok(reply(Status::RateLimited, None)),
                }
            }
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
        let last_seen = last_seen.unwrap_or_else(|| chrono::Utc::now().timestamp());
        match self.store.put_profile_seen(record, last_seen) {
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
    /// Valid and applied: forward it.
    Accept,
    /// Valid but not applied: do not forward it.
    Ignore,
    /// Invalid: penalize the sender.
    Reject,
    /// A proof of work not cached yet: check it off the loop, then apply the message again.
    Pow(PowCheck),
}

/// What catch-up did with one record.
#[derive(Debug, PartialEq, Eq)]
pub enum Caught {
    /// The record is indexed.
    Stored,
    /// The record is not indexed.
    Dropped,
    /// A proof of work not cached yet: check it off the loop, then apply the record again.
    Pow(PowCheck),
}

#[path = "service/search.rs"]
mod search_role;
pub use search_role::{run_get, run_query, search_key, Search};

#[path = "service/mailbox.rs"]
mod mailbox_role;
pub(crate) use mailbox_role::{is_signal, Held};

#[path = "service/revoke.rs"]
mod revoke_role;

#[path = "service/media.rs"]
mod media_role;
pub(crate) use media_role::media_upload_status;

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

/// A stable pseudonym of an IP group: the username leaks no address, and rotating `turn.secret`
/// renews every pseudonym.
fn turn_user(turn: &Turn, group: &str) -> String {
    hex(&turn
        .secret
        .hmac_sha1(format!("ip-group:{group}").as_bytes())[..8])
}

/// TURN REST credentials as coturn's `use-auth-secret` checks them: username
/// `<expiry>:<user>`, password base64(HMAC-SHA1(secret, username)).
fn turn_credentials(turn: &Turn, user: &str, now: u64) -> TurnCredentials {
    use base64::Engine;
    let expires = now + u64::from(turn.credential_minutes) * 60;
    let username = format!("{expires}:{user}");
    let mac = turn.secret.hmac_sha1(username.as_bytes());
    TurnCredentials {
        password: base64::engine::general_purpose::STANDARD.encode(mac),
        username,
        urls: turn.urls.clone(),
        expires,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "service/clock.rs"]
mod clock_tests;

#[cfg(test)]
mod tests;
