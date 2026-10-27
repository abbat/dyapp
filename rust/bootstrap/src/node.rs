//! The libp2p request loop of a node: answers the node protocol and deletes expired envelopes.

mod delivery;
mod identified;
mod jobs;
mod open;
mod refill;
mod replicas;
mod replies;
mod report;
mod republish;
mod run;
use delivery::{after_lookup, after_mailbox, push, send_envelopes};
pub use open::{open, Opened};
use report::{state, status};
pub use run::run;

use crate::config::{Limits, Role};
use crate::deny::DenyStore;
use crate::maintenance::RebuildBudget;
use crate::media_repair::{Repair as MediaRepair, Schedule as MediaRepairSchedule};
use crate::rate_limit::PeerRateLimiter;
use crate::replication::MediaManifestExt;
use crate::search::SEARCH_TOPICS;
use crate::service::{
    run_get, run_query, search_key, Caught, Peer, Search, Service, Verdict, SHED_MEDIA,
};
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto::{self, mailbox_request, MailboxRequest, SearchResponse, Status};
use dyapp_p2p_net::{
    build_configured_swarm, replica_key, Behaviour, BehaviourEvent, DialGuard, IncomingGroupLimits,
    Mode, SwarmOptions, REPLICAS, SEARCH_KEY,
};
use libp2p::connection_limits::ConnectionLimits;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{
    Event, Message, OutboundFailure, OutboundRequestId, ResponseChannel,
};
use libp2p::swarm::{ConnectionId, DialError, ListenError, SwarmEvent};
use libp2p::{gossipsub, identify, kad};
use libp2p::{Multiaddr, PeerId, Swarm};
use prost::Message as _;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

/// The public node swarm with the connection and stream limits from the config. Store Kademlia
/// is the key space of the store role: a node without it is a DHT client and serves no protocol, so it
/// is never picked as a replica. New connections are refused above `max_memory_mb`.
pub fn swarm(
    keypair: Keypair,
    limits: &Limits,
    roles: &[Role],
) -> anyhow::Result<Swarm<Behaviour>> {
    swarm_with_wire(keypair, limits, roles, Arc::default())
}

/// Builds the node swarm with counters shared with its service.
pub fn swarm_with_wire(
    keypair: Keypair,
    limits: &Limits,
    roles: &[Role],
    wire: Arc<dyapp_p2p_net::WireBytes>,
) -> anyhow::Result<Swarm<Behaviour>> {
    swarm_with_protocols(
        keypair,
        limits,
        roles,
        wire,
        Arc::new(dyapp_p2p_net::Protocols::public()),
        false,
        dyapp_p2p_net::DialFilter::default(),
        None,
    )
}

/// Builds a node with protocol IDs shared with its service; config selection is separate.
/// Outbound dials always pass the peer-proof gate; `guard_dials` adds the IP and group limits,
/// and `dial_filter` refuses its ranges. `relay` serves the circuit relay v2 hop protocol.
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is a separate config part"
)]
pub fn swarm_with_protocols(
    keypair: Keypair,
    limits: &Limits,
    roles: &[Role],
    wire: Arc<dyapp_p2p_net::WireBytes>,
    protocols: Arc<dyapp_p2p_net::Protocols>,
    guard_dials: bool,
    dial_filter: dyapp_p2p_net::DialFilter,
    relay: Option<libp2p::relay::Config>,
) -> anyhow::Result<Swarm<Behaviour>> {
    let mode = if roles.contains(&Role::Store) {
        Mode::Auto
    } else {
        Mode::Client
    };
    let connections = ConnectionLimits::default()
        .with_max_established(Some(limits.max_connections))
        .with_max_pending_incoming(Some(limits.max_connections))
        .with_max_established_per_peer(Some(limits.max_connections_per_peer));
    let memory = match limits.max_memory_mb {
        0 => usize::MAX,
        mb => usize::try_from(mb.saturating_mul(1 << 20)).unwrap_or(usize::MAX),
    };
    let search = roles.contains(&Role::Search);
    build_configured_swarm(
        keypair,
        SwarmOptions {
            mode,
            connection_limits: connections,
            max_streams: limits.max_streams,
            max_memory_bytes: memory,
            search,
            wire,
            protocols,
            incoming_groups: Some(IncomingGroupLimits::new(
                limits.max_connections_per_ip_group,
                limits.ipv4_prefix,
                limits.ipv6_prefix,
            )?),
            dial_guard: Some(DialGuard::new(guard_dials)),
            dial_filter,
            relay,
        },
    )
}

/// The IP group of a remote address: its first IP masked to `ipv4_prefix` / `ipv6_prefix` bits,
/// an IPv4-mapped IPv6 address as IPv4.
/// ponytail: a relayed connection gets the relay's group; per-hop groups need the circuit's
/// source address, which relays do not pass on.
pub fn ip_group(address: &Multiaddr, limits: &Limits) -> String {
    let ip = address.iter().find_map(|protocol| match protocol {
        Protocol::Ip4(ip) => Some(IpAddr::V4(ip)),
        Protocol::Ip6(ip) => Some(IpAddr::V6(ip)),
        _ => None,
    });
    ip.map_or_else(
        || address.to_string(),
        |ip| masked(ip, limits.ipv4_prefix, limits.ipv6_prefix),
    )
}

/// `ip` masked to its leading `ipv4` / `ipv6` bits as `ip/bits`, an IPv4-mapped IPv6 address
/// as IPv4.
fn masked(ip: IpAddr, ipv4: u8, ipv6: u8) -> String {
    let mask = |bits: u8, width: u8| u128::MAX.checked_shl(u32::from(width - bits)).unwrap_or(0);
    match ip.to_canonical() {
        IpAddr::V4(ip) => {
            let ip = u128::from(u32::from(ip)) & mask(ipv4, 32);
            format!("{}/{ipv4}", std::net::Ipv4Addr::from(ip as u32))
        }
        IpAddr::V6(ip) => {
            let ip = u128::from(ip) & mask(ipv6, 128);
            format!("{}/{ipv6}", std::net::Ipv6Addr::from(ip))
        }
    }
}

/// Peers saved by the last run; a missing file or a damaged line is skipped.
pub fn cached_peers(path: &Path) -> Vec<Multiaddr> {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines().filter_map(|line| line.parse().ok()).collect()
}

/// Outbound peers kept as anchors (ADR 0008).
pub const ANCHORS: usize = 3;

/// Puts `peer`, dialled at `address`, first among the anchors and keeps the [`ANCHORS`] latest
/// distinct peers.
fn remember_anchor(anchors: &mut Vec<Multiaddr>, peer: PeerId, address: &Multiaddr) {
    let mut address = address.clone();
    if !matches!(address.iter().last(), Some(Protocol::P2p(_))) {
        address.push(Protocol::P2p(peer));
    }
    anchors.retain(|a| a.iter().last() != Some(Protocol::P2p(peer)));
    anchors.insert(0, address);
    anchors.truncate(ANCHORS);
}

fn save_peers(path: &Path, peers: &[Multiaddr]) -> std::io::Result<()> {
    fs::write(
        path,
        peers.iter().map(|p| format!("{p}\n")).collect::<String>(),
    )
}

/// Stores `peer`'s deny list if its node key signed it.
fn keep_deny_list(deny: &DenyStore, peer: &PeerId, record: &SignedRecord) {
    let Some(list) = crate::deny::verify(record, peer) else {
        return tracing::warn!(%peer, "deny list with a bad signature dropped");
    };
    match deny.keep_received(&peer.to_string(), list.time, record) {
        Ok(true) => tracing::info!(%peer, entries = list.entries.len(), "deny list received"),
        Ok(false) => {}
        Err(error) => tracing::error!(%peer, %error, "deny list not stored"),
    }
}

/// Re-reads the deny list when `dyappd deny` changed it since `version`; on an error the old
/// list stays.
fn reload_deny(service: &Service, deny: &DenyStore, version: &mut Option<i64>) {
    let current = match deny.version() {
        Ok(current) if Some(current) == *version => return,
        Ok(current) => current,
        Err(error) => return tracing::error!(%error, "deny list not checked"),
    };
    match deny.entries() {
        Ok(entries) => {
            let mut old = service.deny.write().unwrap_or_else(PoisonError::into_inner);
            let added = entries.difference(&old).count();
            let removed = old.difference(&entries).count();
            // Hold the write lock through purge: replica writes hold its read lock until stored.
            let mut purged = true;
            if let Some(media) = &service.media {
                for entry in &entries {
                    if entry.len() != 64 || !entry.bytes().all(|b| b.is_ascii_hexdigit()) {
                        continue;
                    }
                    let Ok(hash) = (0..64)
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&entry[i..i + 2], 16))
                        .collect::<Result<Vec<_>, _>>()
                    else {
                        continue;
                    };
                    if let Err(error) = media.purge(&hash) {
                        tracing::error!(hash = %entry, %error, "deny-listed media not purged");
                        purged = false;
                    }
                }
            }
            tracing::info!(entries = entries.len(), added, removed, "deny list loaded");
            let key = service.deny_signer.as_ref();
            *service
                .shared_deny
                .write()
                .unwrap_or_else(PoisonError::into_inner) =
                key.map(|key| crate::deny::sign(key, &entries));
            *old = entries;
            *version = purged.then_some(current);
        }
        Err(error) => tracing::error!(%error, "deny list not loaded, the old one stays"),
    }
}

/// The request's peer, with the IP group of the connection it came on: empty while its IP is
/// unknown, which every group limit refuses.
fn peer(
    swarm: &Swarm<Behaviour>,
    service: &Service,
    peer: PeerId,
    connection: ConnectionId,
) -> Peer {
    let ip = swarm.behaviour().endpoints.ip(connection);
    let l = &service.config.limits;
    let group = |ipv4, ipv6| ip.map(|ip| masked(ip, ipv4, ipv6)).unwrap_or_default();
    Peer {
        id: peer.to_string(),
        group: group(l.ipv4_prefix, l.ipv6_prefix),
        coarse: group(l.coarse_ipv4_prefix, l.coarse_ipv6_prefix),
    }
}

/// Spreads an accepted publish or heartbeat to the topic's other search nodes.
fn spread(
    swarm: &mut Swarm<Behaviour>,
    gossip: Option<([u8; 32], Vec<u8>)>,
    response: &SearchResponse,
    service: &Service,
) {
    let (Some((key, bytes)), true) = (gossip, response.status == i32::from(Status::Ok)) else {
        return;
    };
    let topic = service.protocols.search_topic(crate::search::topic(&key));
    if let Some(gossip) = swarm.behaviour_mut().gossip.as_mut() {
        let n = bytes.len();
        match gossip.publish(topic, bytes) {
            Ok(_) => service
                .wire
                .add(dyapp_p2p_net::WireBytes::GOSSIP, dyapp_p2p_net::Dir::Out, n),
            Err(error) => tracing::debug!(%error, "search gossip not sent"),
        }
    }
}

/// Reports a gossip verdict; a rejected message strikes the peer that propagated it.
fn judge(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    id: &gossipsub::MessageId,
    source: &PeerId,
    verdict: Verdict,
) {
    let acceptance = match verdict {
        Verdict::Accept => gossipsub::MessageAcceptance::Accept,
        Verdict::Reject => gossipsub::MessageAcceptance::Reject,
        Verdict::Ignore | Verdict::Pow(_) => gossipsub::MessageAcceptance::Ignore,
    };
    if let Some(gossip) = swarm.behaviour_mut().gossip.as_mut() {
        gossip.report_message_validation_result(id, source, acceptance);
    }
    if matches!(verdict, Verdict::Reject) {
        // A strike counts by peer ID; gossip has no connection to take a group from.
        service.strike(&Peer {
            id: source.to_string(),
            group: String::new(),
            coarse: String::new(),
        });
        drop_banned(swarm, service, *source);
    }
}

/// Closes every connection of a peer banned for misbehaviour; it may reconnect only to be
/// dropped again until the ban ends.
fn drop_banned(swarm: &mut Swarm<Behaviour>, service: &Service, peer: PeerId) {
    if service.reputation.banned(&peer.to_string()) {
        let _ = swarm.disconnect_peer_id(peer);
    }
}

/// Connections that fetched a mailbox with `watch`, by mailbox address.
type Watchers = HashMap<Vec<u8>, Vec<(PeerId, ConnectionId)>>;

/// A client write stays in memory until two distinct holders confirm it.
const MAX_PENDING_PUTS: usize = 64;
const PUT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
enum PutRecord {
    /// An envelope and its source group.
    Mailbox(SignedRecord, Vec<u8>),
    Profile(SignedRecord),
    KeyPackage(SignedRecord),
    Media(proto::MediaReplicaPut),
    Sharded(proto::MediaReplicaPut, Arc<Vec<Vec<u8>>>),
}

enum PutReply {
    Mailbox(libp2p::request_response::ResponseChannel<proto::MailboxResponse>),
    Profile(libp2p::request_response::ResponseChannel<proto::ProfileResponse>),
    KeyPackage(libp2p::request_response::ResponseChannel<proto::KeyPackageResponse>),
    Media(libp2p::request_response::ResponseChannel<proto::MediaResponse>),
}

impl PutReply {
    fn send(
        self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        status: Status,
        record: Option<SignedRecord>,
    ) {
        match self {
            Self::Mailbox(channel) => {
                let response = proto::MailboxResponse {
                    status: status.into(),
                    ..Default::default()
                };
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .mailbox
                    .send_response(channel, response);
            }
            Self::Profile(channel) => {
                let response = proto::ProfileResponse {
                    status: status.into(),
                    record,
                    ..Default::default()
                };
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .profile
                    .send_response(channel, response);
            }
            Self::KeyPackage(channel) => {
                let response = proto::KeyPackageResponse {
                    status: status.into(),
                    ..Default::default()
                };
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .key_package
                    .send_response(channel, response);
            }
            Self::Media(channel) => {
                let response = proto::MediaResponse {
                    status: status.into(),
                    ..Default::default()
                };
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm.behaviour_mut().media.send_response(channel, response);
            }
        }
    }
}

struct PendingPut {
    record: PutRecord,
    reply: Option<PutReply>,
    deadline: Instant,
    sent: HashSet<PeerId>,
    stored: HashSet<PeerId>,
    required: usize,
    stale: Option<SignedRecord>,
    shard_sent: HashSet<(usize, PeerId)>,
    shard_stored: HashSet<usize>,
}

impl PendingPut {
    fn new(record: PutRecord, reply: Option<PutReply>, isolated: bool) -> Self {
        Self {
            record,
            reply,
            deadline: Instant::now() + PUT_TIMEOUT,
            sent: HashSet::new(),
            stored: HashSet::new(),
            required: if isolated { 1 } else { 2 },
            stale: None,
            shard_sent: HashSet::new(),
            shard_stored: HashSet::new(),
        }
    }

    fn confirm(&mut self, peer: PeerId, status: i32, held: Option<SignedRecord>) {
        if status == i32::from(Status::Ok) {
            self.stored.insert(peer);
        } else if let (PutRecord::Profile(requested), Some(held)) = (&self.record, held) {
            if status != i32::from(Status::Stale) || held.public_key != requested.public_key {
                return;
            }
            let Ok(verified) = dyapp_profile::verify(&held) else {
                return;
            };
            let Ok(version) = dyapp_profile::stored_version(requested) else {
                return;
            };
            if verified.profile.version >= version {
                self.stored.insert(peer);
                if self.stale.as_ref().is_none_or(|old| {
                    dyapp_profile::stored_version(old).is_ok_and(|v| v < verified.profile.version)
                }) {
                    self.stale = Some(held);
                }
            }
        }
    }

    fn result(&self, now: Instant) -> Option<Status> {
        if now >= self.deadline {
            Some(Status::Full)
        } else if self.stored.len() >= self.required
            && match &self.record {
                PutRecord::Sharded(replica, _) => {
                    self.shard_stored.len()
                        >= replica.manifest.as_ref().unwrap().data_shards().unwrap()
                }
                _ => true,
            }
        {
            Some(if self.stale.is_some() {
                Status::Stale
            } else {
                Status::Ok
            })
        } else {
            None
        }
    }
}

struct MediaRead {
    manifest: proto::MediaManifest,
    get: proto::GetMedia,
    reply: libp2p::request_response::ResponseChannel<proto::MediaResponse>,
    deadline: Instant,
    shards: HashMap<usize, Vec<u8>>,
}

#[derive(Clone, Copy)]
enum MediaJob {
    Put(u64),
    ShardPut(u64, usize),
    ShardGet(u64, usize),
    Repair(u64, usize),
}

/// Chooses the actual closest holder, including this node, before applying trust policy.
fn holder(me: PeerId, key: &[u8], nodes: &[kad::PeerInfo]) -> PeerId {
    let target = kad::KBucketKey::new(key.to_vec());
    nodes
        .iter()
        .map(|node| node.peer_id)
        .chain(std::iter::once(me))
        .min_by_key(|peer| kad::KBucketKey::from(*peer).distance(&target))
        .unwrap()
}

/// A lookup of a replica key and what follows it.
enum Lookup {
    MediaRepair(u64, usize, u8),
    Put(u64),
    ShardPut(u64, usize),
    ShardGet(u64, usize),
    /// A device's ack or revoke, forwarded verbatim to the closest node.
    Forward(mailbox_request::Request),
    /// A mailbox whose inventory goes to the closest node.
    Repair(Vec<u8>),
    ProfileRepair(Arc<SignedRecord>),
}

/// Local trust in peers as replica targets (ADR 0008), never shared. A peer is trusted once this
/// node first connected to it at least `delay` ago and while its score is not negative: +1 per
/// answer to this node's mailbox requests and per signed `replica_put`, `replica_ack` or
/// `replica_revoke` accepted from it, -1 per mailbox request of this node it failed. Bad
/// signatures and floods are strikes of the separate ban score in `Service`.
/// ponytail: in memory only, so after a restart no peer gets replicas for `delay`.
#[derive(Default)]
struct Trust {
    delay: Duration,
    seen: HashMap<PeerId, Instant>,
    score: HashMap<PeerId, i64>,
}

impl Trust {
    fn trusted(&self, peer: &PeerId) -> bool {
        let old = |seen: &Instant| seen.elapsed() >= self.delay;
        let seen = self.delay.is_zero() || self.seen.get(peer).is_some_and(old);
        seen && self.score.get(peer).copied().unwrap_or(0) >= 0
    }

    fn add(&mut self, peer: PeerId, points: i64) {
        *self.score.entry(peer).or_default() += points;
    }

    fn forget(&mut self, peer: &PeerId) {
        self.seen.remove(peer);
        self.score.remove(peer);
    }

    /// Forgets the peers `keep` drops.
    fn retain(&mut self, keep: impl Fn(&PeerId) -> bool) {
        self.seen.retain(|peer, _| keep(peer));
        self.score.retain(|peer, _| keep(peer));
    }
}

const PROFILE_REPAIR_INTERVAL: Duration = Duration::from_secs(3600);
const MAX_PROFILE_REPAIRS: usize = 4096;

const REPLICA_UNITS_PER_SECOND: u32 = 50;

/// Shared token bucket per destination; local refusal is not remote misbehaviour.
struct ReplicaCap {
    limiter: PeerRateLimiter,
    dropped: u64,
}

impl Default for ReplicaCap {
    fn default() -> Self {
        Self::new(REPLICA_UNITS_PER_SECOND)
    }
}

impl ReplicaCap {
    fn new(units_per_second: u32) -> Self {
        Self {
            limiter: PeerRateLimiter::new(units_per_second),
            dropped: 0,
        }
    }

    fn allow(&mut self, to: &PeerId, units: u32) -> bool {
        let allowed = self.limiter.check_units(&to.to_string(), units);
        self.dropped += u64::from(!allowed);
        allowed
    }

    fn allow_media(&mut self, to: &PeerId, request: &proto::media_request::Request) -> bool {
        !matches!(
            request,
            proto::media_request::Request::ReplicaPut(_)
                | proto::media_request::Request::ShardPut(_)
        ) || self.allow(to, 1)
    }

    fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }
}

type MediaRepairAnswer = (ResponseChannel<proto::MediaResponse>, proto::MediaResponse);

/// Node-to-node replication, ack forwarding and repair.
#[derive(Default)]
struct Replicas {
    trust: Trust,
    cap: ReplicaCap,
    next_put: u64,
    puts: HashMap<u64, PendingPut>,
    mailbox_puts: HashMap<OutboundRequestId, u64>,
    profile_puts: HashMap<OutboundRequestId, u64>,
    keypackage_puts: HashMap<OutboundRequestId, u64>,
    media_puts: HashMap<OutboundRequestId, u64>,
    shard_puts: HashMap<OutboundRequestId, (u64, usize)>,
    shard_gets: HashMap<OutboundRequestId, (u64, usize)>,
    media_reads: HashMap<u64, MediaRead>,
    media_repairs: HashMap<u64, (MediaRepair, Option<MediaRepairAnswer>)>,
    media_repaired: MediaRepairSchedule,
    media_repair_requests: HashMap<OutboundRequestId, (u64, usize, u64)>,
    media_queue: std::collections::VecDeque<(PeerId, MediaJob)>,
    media_outgoing: HashMap<OutboundRequestId, PeerId>,
    /// Most recent media write and its distinct confirmed holders, including background replies.
    media_holders: (u64, usize),
    lookups: HashMap<kad::QueryId, Lookup>,
    /// Inventories sent and their mailbox.
    inventories: HashMap<OutboundRequestId, Vec<u8>>,
    /// Mailboxes repaired since the last hourly cleanup.
    /// ponytail: cleared hourly, so a mailbox may be repaired twice within an hour.
    repaired: HashSet<Vec<u8>>,
    profile_repaired: HashMap<Vec<u8>, (Instant, HashSet<PeerId>)>,
    profile_inventories: HashMap<OutboundRequestId, Arc<SignedRecord>>,
    republish: republish::Republish,
}

/// A store request run off the swarm loop: it gets the connection's mailbox challenge.
type Job = Box<dyn FnOnce(&Service, &mut Option<[u8; 32]>) -> StoreDone + Send>;

/// What a store job hands back to the swarm loop to reply and replicate.
enum StoreDone {
    ProfilePublish(
        ResponseChannel<proto::ProfileResponse>,
        SignedRecord,
        crate::Result<proto::ProfileResponse>,
    ),
    /// The stored profile comes with an accepted heartbeat, to repair its replicas.
    Profile(
        ResponseChannel<proto::ProfileResponse>,
        crate::Result<proto::ProfileResponse>,
        Option<SignedRecord>,
    ),
    KeyPackagePublish(
        ResponseChannel<proto::KeyPackageResponse>,
        SignedRecord,
        crate::Result<proto::KeyPackageResponse>,
    ),
    KeyPackage(
        ResponseChannel<proto::KeyPackageResponse>,
        crate::Result<proto::KeyPackageResponse>,
    ),
    MediaPut(
        ResponseChannel<proto::MediaResponse>,
        crate::Result<std::result::Result<proto::MediaReplicaPut, Status>>,
    ),
    Media {
        channel: ResponseChannel<proto::MediaResponse>,
        result: crate::Result<proto::MediaResponse>,
        keep: Option<SignedRecord>,
        have: Option<Vec<proto::MediaPart>>,
        /// The manifest of a blob the get did not find whole or cached here, with the get.
        manifest: Option<(proto::MediaManifest, proto::GetMedia)>,
    },
    MailboxPut(
        ResponseChannel<proto::MailboxResponse>,
        SignedRecord,
        Vec<u8>,
        crate::Result<proto::MailboxResponse>,
    ),
    Mailbox(
        ResponseChannel<proto::MailboxResponse>,
        Option<mailbox_request::Request>,
        crate::Result<proto::MailboxResponse>,
    ),
    Inventory(
        ResponseChannel<proto::InventoryResponse>,
        crate::Result<proto::InventoryResponse>,
    ),
    Digest(
        ResponseChannel<proto::DigestResponse>,
        crate::Result<proto::DigestResponse>,
    ),
}

/// A finished store job; `done` is `None` when it panicked, which drops the reply.
struct Finished {
    peer: PeerId,
    connection: ConnectionId,
    nonce: Option<[u8; 32]>,
    done: Option<StoreDone>,
}

/// Queues a store job; a full queue drops it, which fails the request on the client.
fn enqueue(
    queue: &mut jobs::FairQueue<(ConnectionId, Job)>,
    peer: PeerId,
    connection: ConnectionId,
    job: Job,
) {
    if queue.push(peer, (connection, job)).is_err() {
        tracing::debug!(%peer, "store queue full, request dropped");
    }
}

/// Starts the queued store jobs that may run now.
fn dispatch(
    queue: &mut jobs::FairQueue<(ConnectionId, Job)>,
    nonces: &mut HashMap<ConnectionId, [u8; 32]>,
    service: &Arc<Service>,
    finished: &tokio::sync::mpsc::UnboundedSender<Finished>,
) {
    while let Some((peer, (connection, job))) = queue.next() {
        let (service, finished) = (service.clone(), finished.clone());
        let mut nonce = nonces.remove(&connection);
        tokio::task::spawn_blocking(move || {
            let run = std::panic::AssertUnwindSafe(|| job(&service, &mut nonce));
            let done = std::panic::catch_unwind(run).ok();
            let _ = finished.send(Finished {
                peer,
                connection,
                nonce,
                done,
            });
        });
    }
}

/// Up to two distinct mesh peers from `pick` on that are asked for one topic's inventory, so one
/// peer that keeps a subset or withholds records cannot leave a silent hole.
// ponytail: keys missing from both inventories are fetched twice; dedupe if catch-up traffic shows.
fn catch_up_peers(peers: &[PeerId], pick: u32) -> Vec<PeerId> {
    let start = pick as usize % peers.len().max(1);
    (0..peers.len().min(2))
        .map(|i| peers[(start + i) % peers.len()])
        .collect()
}

/// Sends catch-up gets, at most four in flight per peer, each of up to 50 keys, as the shared
/// rebuild budget allows. Returns when the budget allows the next get.
fn send_catch_up(
    swarm: &mut Swarm<Behaviour>,
    budget: &RebuildBudget,
    queue: &mut HashMap<PeerId, VecDeque<[u8; 32]>>,
    asked: &mut HashMap<OutboundRequestId, (PeerId, Vec<[u8; 32]>)>,
) -> Option<tokio::time::Instant> {
    queue.retain(|_, keys| !keys.is_empty());
    for (peer, keys) in queue.iter_mut() {
        let mut in_flight = asked.values().filter(|(p, _)| p == peer).count();
        while in_flight < 4 && !keys.is_empty() {
            let chunk: Vec<[u8; 32]> = keys.drain(..keys.len().min(50)).collect();
            let request = proto::SearchRequest {
                request: Some(proto::search_request::Request::Get(proto::SearchGet {
                    keys: chunk.iter().map(|k| k.to_vec()).collect(),
                    puzzle: None,
                })),
            };
            let id = swarm.behaviour_mut().search.send_request(peer, request);
            asked.insert(id, (*peer, chunk));
            in_flight += 1;
            // A reply holds at most 1 MiB; the next get waits until the budget covers it.
            let now = tokio::time::Instant::now();
            let next = budget.reserve(now, 1 << 20, "profiles-idx.db", 0);
            if next > now {
                return Some(next);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
