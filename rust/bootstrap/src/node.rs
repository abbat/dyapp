//! The libp2p request loop of a node: answers the node protocol and deletes expired envelopes.

use crate::config::{Limits, Role};
use crate::deny::DenyStore;
use crate::media_repair::{Repair as MediaRepair, Schedule as MediaRepairSchedule};
use crate::replication::MediaManifestExt;
use crate::service::{Peer, Service, SHED_MEDIA};
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto::{self, mailbox_request, MailboxRequest, Status};
use dyapp_p2p_net::{
    build_limited_swarm, replica_key, Behaviour, BehaviourEvent, Mode, KAD_PROTOCOL, REPLICAS,
};
use libp2p::connection_limits::ConnectionLimits;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{Event, Message, OutboundFailure, OutboundRequestId};
use libp2p::swarm::{ConnectionId, ListenError, SwarmEvent};
use libp2p::{identify, kad};
use libp2p::{Multiaddr, PeerId, Swarm};
use prost::Message as _;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The node's swarm with the connection and stream limits from the config. `/dyapp/kad` is the
/// key space of the store role: a node without it is a DHT client and serves no protocol, so it
/// is never picked as a replica. New connections are refused above `max_memory_mb`.
pub fn swarm(
    keypair: Keypair,
    limits: &Limits,
    roles: &[Role],
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
    build_limited_swarm(keypair, mode, connections, limits.max_streams, memory)
}

/// The IP group of a remote address: its first IP masked to `ipv4_prefix` / `ipv6_prefix` bits.
/// ponytail: a relayed connection gets the relay's group; per-hop groups need the circuit's
/// source address, which relays do not pass on.
pub fn ip_group(address: &Multiaddr, limits: &Limits) -> String {
    let mask = |bits: u8, width: u8| u128::MAX.checked_shl(u32::from(width - bits)).unwrap_or(0);
    for protocol in address.iter() {
        match protocol {
            Protocol::Ip4(ip) => {
                let ip = u128::from(u32::from(ip)) & mask(limits.ipv4_prefix, 32);
                return format!(
                    "{}/{}",
                    std::net::Ipv4Addr::from(ip as u32),
                    limits.ipv4_prefix
                );
            }
            Protocol::Ip6(ip) => {
                let ip = u128::from(ip) & mask(limits.ipv6_prefix, 128);
                return format!("{}/{}", std::net::Ipv6Addr::from(ip), limits.ipv6_prefix);
            }
            _ => {}
        }
    }
    address.to_string()
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
fn reload_deny(service: &mut Service, deny: &DenyStore, version: &mut Option<i64>) {
    let current = match deny.version() {
        Ok(current) if Some(current) == *version => return,
        Ok(current) => current,
        Err(error) => return tracing::error!(%error, "deny list not checked"),
    };
    match deny.entries() {
        Ok(entries) => {
            let added = entries.difference(&service.deny).count();
            let removed = service.deny.difference(&entries).count();
            tracing::info!(entries = entries.len(), added, removed, "deny list loaded");
            let key = service.deny_signer.as_ref();
            service.shared_deny = key.map(|key| crate::deny::sign(key, &entries));
            service.deny = entries;
            *version = Some(current);
        }
        Err(error) => tracing::error!(%error, "deny list not loaded, the old one stays"),
    }
}

/// The request's peer, with the IP group of the connection it came on.
fn peer(peer: PeerId, groups: &HashMap<ConnectionId, String>, connection: ConnectionId) -> Peer {
    Peer {
        id: peer.to_string(),
        group: groups.get(&connection).cloned().unwrap_or_default(),
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
    Mailbox(SignedRecord),
    Profile(SignedRecord),
    Media(proto::MediaReplicaPut),
    Sharded(proto::MediaReplicaPut, Arc<Vec<Vec<u8>>>),
}

enum PutReply {
    Mailbox(libp2p::request_response::ResponseChannel<proto::MailboxResponse>),
    Profile(libp2p::request_response::ResponseChannel<proto::ProfileResponse>),
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
    /// A device's ack, forwarded to the closest node.
    Ack(SignedRecord),
    /// A mailbox whose inventory goes to the closest node.
    Repair(Vec<u8>),
    ProfileRepair(Arc<SignedRecord>),
}

/// Local trust in peers as replica targets (ADR 0008), never shared. A peer is trusted once this
/// node first connected to it at least `delay` ago and while its score is not negative: +1 per
/// answer to this node's mailbox requests and per signed `replica_put` or `replica_ack` accepted
/// from it, -1 per mailbox request of this node it failed. Bad signatures and floods are strikes
/// of the separate ban score in `Service`.
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

    /// Forgets the peers `keep` drops.
    fn retain(&mut self, keep: impl Fn(&PeerId) -> bool) {
        self.seen.retain(|peer, _| keep(peer));
        self.score.retain(|peer, _| keep(peer));
    }
}

const PROFILE_REPAIR_INTERVAL: Duration = Duration::from_secs(3600);
const MAX_PROFILE_REPAIRS: usize = 4096;

/// Node-to-node replication, ack forwarding and repair.
#[derive(Default)]
struct Replicas {
    trust: Trust,
    next_put: u64,
    puts: HashMap<u64, PendingPut>,
    mailbox_puts: HashMap<OutboundRequestId, u64>,
    profile_puts: HashMap<OutboundRequestId, u64>,
    media_puts: HashMap<OutboundRequestId, u64>,
    shard_puts: HashMap<OutboundRequestId, (u64, usize)>,
    shard_gets: HashMap<OutboundRequestId, (u64, usize)>,
    media_reads: HashMap<u64, MediaRead>,
    media_repairs: HashMap<
        u64,
        (
            MediaRepair,
            libp2p::request_response::ResponseChannel<proto::MediaResponse>,
            proto::MediaResponse,
        ),
    >,
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
}

impl Replicas {
    fn repair_media(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        record: SignedRecord,
        reply: libp2p::request_response::ResponseChannel<proto::MediaResponse>,
        mut response: proto::MediaResponse,
        now: Instant,
    ) {
        let keep = proto::MediaKeep::decode(record.payload.as_slice()).unwrap();
        for hash in &keep.hashes {
            if let Some(missing) = self.media_repaired.result(hash, now) {
                response.missing.retain(|h| h != hash);
                if missing {
                    response.missing.push(hash.clone());
                }
            }
        }
        if service.traffic.second() >= SHED_MEDIA || self.media_repairs.len() >= 4 {
            service.traffic.add(response.encoded_len() as u64);
            let _ = swarm.behaviour_mut().media.send_response(reply, response);
            return;
        }
        let owner =
            dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec();
        let hashes = self.media_repaired.select(owner, &keep.hashes, now);
        if hashes.is_empty() {
            service.traffic.add(response.encoded_len() as u64);
            let _ = swarm.behaviour_mut().media.send_response(reply, response);
            return;
        }
        let mut repair = match MediaRepair::new(service, record, hashes, now) {
            Ok(repair) => repair,
            Err(error) => {
                tracing::error!(%error, "media repair initialization failed");
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm.behaviour_mut().media.send_response(reply, response);
                return;
            }
        };
        let id = self.next_put;
        self.next_put = self.next_put.wrapping_add(1);
        let isolated = dyapp_p2p_net::known_peers(swarm).is_empty();
        let hashes: Vec<_> = repair.hashes().map(<[u8]>::to_vec).collect();
        for (blob, hash) in hashes.iter().enumerate() {
            for index in repair.keys(blob) {
                if isolated {
                    repair.holder(blob, index, Some(*swarm.local_peer_id()));
                } else {
                    let query = swarm
                        .behaviour_mut()
                        .kad
                        .get_closest_peers(replica_key(hash, index));
                    self.lookups
                        .insert(query, Lookup::MediaRepair(id, blob, index));
                }
            }
        }
        self.media_repairs.insert(id, (repair, reply, response));
        self.finish_puts(swarm, service, now);
    }

    fn media_inventory(
        &self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        peer: PeerId,
        items: &[proto::MediaPart],
    ) -> crate::Result<proto::MediaResponse> {
        let mut allowed = Vec::new();
        if self.trust.trusted(&peer) {
            let mut near = HashMap::new();
            for part in items {
                let near = *near.entry(part.hash.clone()).or_insert_with(|| {
                    (0..10).any(|i| {
                        let key = kad::KBucketKey::new(replica_key(&part.hash, i));
                        let near = swarm
                            .behaviour_mut()
                            .kad
                            .get_closest_local_peers(&key)
                            .take(REPLICAS.into())
                            .any(|p| *p.preimage() == peer);
                        near
                    })
                });
                if near {
                    allowed.push(part.clone());
                }
            }
        }
        let response = service.media_inventory(&allowed)?;
        if service
            .traffic
            .try_add(response.encoded_len() as u64, SHED_MEDIA)
        {
            Ok(response)
        } else {
            Ok(proto::MediaResponse {
                status: Status::RateLimited.into(),
                ..Default::default()
            })
        }
    }

    fn finish_media_repairs(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        now: Instant,
    ) {
        let mut done = Vec::new();
        for (id, (repair, _, _)) in &mut self.media_repairs {
            if now < repair.deadline {
                repair.advance(service, *swarm.local_peer_id());
                for (peer, job) in repair.queued() {
                    self.media_queue
                        .push_back((peer, MediaJob::Repair(*id, job)));
                }
            }
            if repair.done() || now >= repair.deadline {
                done.push(*id);
            }
        }
        for id in done {
            let (repair, reply, mut response) = self.media_repairs.remove(&id).unwrap();
            for (hash, missing) in repair.results() {
                self.media_repaired.finish(hash, missing);
                response.missing.retain(|h| h.as_slice() != hash);
                if missing {
                    response.missing.push(hash.to_vec());
                }
            }
            service.traffic.add(response.encoded_len() as u64);
            let _ = swarm.behaviour_mut().media.send_response(reply, response);
        }
        for (id, lookup) in &self.lookups {
            if matches!(lookup, Lookup::MediaRepair(repair, _, _) if !self.media_repairs.contains_key(repair))
            {
                if let Some(mut query) = swarm.behaviour_mut().kad.query_mut(id) {
                    query.finish();
                }
            }
        }
        self.lookups.retain(|_, l| !matches!(l, Lookup::MediaRepair(id, _, _) if !self.media_repairs.contains_key(id)));
    }

    fn repair_profile(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        record: SignedRecord,
        now: Instant,
    ) {
        if service.traffic.second() >= SHED_MEDIA || dyapp_p2p_net::known_peers(swarm).is_empty() {
            return;
        }
        let Ok(verified) = dyapp_profile::verify(&record) else {
            return;
        };
        let key =
            dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec();
        self.profile_repaired
            .retain(|_, (at, _)| now < *at + PROFILE_REPAIR_INTERVAL);
        if self.profile_repaired.contains_key(&key)
            || self.profile_repaired.len() >= MAX_PROFILE_REPAIRS
        {
            return;
        }
        let record = match service.store.get_profile(&verified.peer_id) {
            Ok(Some(held))
                if dyapp_profile::stored_version(&held)
                    .is_ok_and(|version| version > verified.profile.version) =>
            {
                held
            }
            Ok(_) => record,
            Err(error) => return tracing::error!(%error, "profile repair failed"),
        };
        self.profile_repaired
            .insert(key.clone(), (now, HashSet::new()));
        let record = Arc::new(record);
        for i in 0..REPLICAS {
            let query = swarm
                .behaviour_mut()
                .kad
                .get_closest_peers(replica_key(&key, i));
            self.lookups
                .insert(query, Lookup::ProfileRepair(record.clone()));
        }
    }

    fn profile_inventory(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        to: PeerId,
        record: Arc<SignedRecord>,
    ) {
        if to == *swarm.local_peer_id() || !self.trust.trusted(&to) {
            return;
        }
        let key =
            dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec();
        let Some((_, sent)) = self.profile_repaired.get_mut(&key) else {
            return;
        };
        if sent.contains(&to) {
            return;
        }
        let request = proto::ProfileRequest {
            request: Some(proto::profile_request::Request::Inventory(
                proto::GetProfile { peer_id: key },
            )),
        };
        if !service
            .traffic
            .try_add(request.encoded_len() as u64, SHED_MEDIA)
        {
            return;
        }
        sent.insert(to);
        let id = swarm.behaviour_mut().profile.send_request(&to, request);
        self.profile_inventories.insert(id, record);
    }

    fn profile_gap(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        to: PeerId,
        record: &SignedRecord,
        response: &proto::ProfileResponse,
    ) -> Option<OutboundRequestId> {
        let version = dyapp_profile::stored_version(record).ok()?;
        if response.status != i32::from(Status::NotFound)
            && !(response.status == i32::from(Status::Ok) && response.version < version)
        {
            return None;
        }
        let request = proto::ProfileRequest {
            request: Some(proto::profile_request::Request::ReplicaPut(record.clone())),
        };
        if self.trust.trusted(&to)
            && service
                .traffic
                .try_add(request.encoded_len() as u64, SHED_MEDIA)
        {
            Some(swarm.behaviour_mut().profile.send_request(&to, request))
        } else {
            None
        }
    }

    fn at_capacity(&self) -> bool {
        self.puts.len() >= MAX_PENDING_PUTS
    }

    fn start_put(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        watchers: &Watchers,
        record: PutRecord,
        reply: PutReply,
    ) {
        if self.at_capacity() {
            reply.send(swarm, service, Status::Full, None);
            return;
        }
        let record = match record {
            PutRecord::Media(mut replica)
                if replica.data.len() > service.config.media.shard_threshold =>
            {
                match crate::replication::media_encode(&replica.data) {
                    Ok((manifest, shards)) => {
                        replica.data.clear();
                        replica.manifest = Some(manifest);
                        PutRecord::Sharded(replica, Arc::new(shards))
                    }
                    Err(error) => {
                        tracing::error!(%error, "media encode failed");
                        reply.send(swarm, service, Status::Invalid, None);
                        return;
                    }
                }
            }
            record => record,
        };
        let shard_count = match &record {
            PutRecord::Sharded(_, shards) => shards.len(),
            _ => 0,
        };
        let isolated = dyapp_p2p_net::known_peers(swarm).is_empty();
        let key = match &record {
            PutRecord::Mailbox(record) => {
                proto::Envelope::decode(record.payload.as_slice())
                    .unwrap()
                    .mailbox
            }
            PutRecord::Profile(record) => {
                dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec()
            }
            PutRecord::Media(replica) => dyapp_identity::sha256(&replica.data).to_vec(),
            PutRecord::Sharded(replica, _) => replica.manifest.as_ref().unwrap().hash.clone(),
        };
        let id = self.next_put;
        self.next_put = self.next_put.wrapping_add(1);
        self.puts
            .insert(id, PendingPut::new(record, Some(reply), isolated));
        if isolated {
            self.put_holder(swarm, service, watchers, id, *swarm.local_peer_id());
        } else {
            for i in 0..REPLICAS {
                let query = swarm
                    .behaviour_mut()
                    .kad
                    .get_closest_peers(replica_key(&key, i));
                self.lookups.insert(query, Lookup::Put(id));
            }
        }
        for index in 0..shard_count {
            if isolated {
                self.shard_holder(swarm, service, id, index, *swarm.local_peer_id());
            } else {
                let query = swarm
                    .behaviour_mut()
                    .kad
                    .get_closest_peers(replica_key(&key, index as u8));
                self.lookups.insert(query, Lookup::ShardPut(id, index));
            }
        }
        self.finish_puts(swarm, service, Instant::now());
    }

    fn put_holder(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        watchers: &Watchers,
        id: u64,
        to: PeerId,
    ) {
        let Some(put) = self.puts.get_mut(&id) else {
            return;
        };
        if !put.sent.insert(to) {
            return;
        }
        if to == *swarm.local_peer_id() {
            match &put.record {
                PutRecord::Mailbox(record) => match service.store_mailbox(record) {
                    Ok(response) => {
                        if response.status == i32::from(Status::Ok) {
                            push(swarm, watchers, vec![record.clone()]);
                        }
                        put.confirm(to, response.status, None);
                    }
                    Err(error) => tracing::error!(%error, "local mailbox replica failed"),
                },
                PutRecord::Profile(record) => match service.store_profile(record) {
                    Ok(response) => put.confirm(to, response.status, response.record),
                    Err(error) => tracing::error!(%error, "local profile replica failed"),
                },
                PutRecord::Media(replica) | PutRecord::Sharded(replica, _) => {
                    match service.store_media(replica) {
                        Ok(response) => put.confirm(to, response.status, None),
                        Err(error) => tracing::error!(%error, "local media replica failed"),
                    }
                }
            }
        } else if self.trust.trusted(&to) {
            match &put.record {
                PutRecord::Mailbox(record) => {
                    let request = MailboxRequest {
                        request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                            envelopes: vec![record.clone()],
                        })),
                    };
                    service.traffic.add(request.encoded_len() as u64);
                    let request_id = swarm.behaviour_mut().mailbox.send_request(&to, request);
                    self.mailbox_puts.insert(request_id, id);
                }
                PutRecord::Profile(record) => {
                    let request = proto::ProfileRequest {
                        request: Some(proto::profile_request::Request::ReplicaPut(record.clone())),
                    };
                    service.traffic.add(request.encoded_len() as u64);
                    let request_id = swarm.behaviour_mut().profile.send_request(&to, request);
                    self.profile_puts.insert(request_id, id);
                }
                PutRecord::Media(replica) | PutRecord::Sharded(replica, _) => {
                    let _ = replica;
                    self.media_queue.push_back((to, MediaJob::Put(id)));
                }
            }
        }
    }

    fn finish_puts(&mut self, swarm: &mut Swarm<Behaviour>, service: &Service, now: Instant) {
        let mut profiles = Vec::new();
        let mut media_written = Vec::new();
        for (id, put) in &mut self.puts {
            if matches!(put.record, PutRecord::Media(_) | PutRecord::Sharded(_, _))
                && *id >= self.media_holders.0
            {
                self.media_holders = (*id, put.stored.len());
            }
            if let Some(status) = put.result(now) {
                if let Some(reply) = put.reply.take() {
                    if matches!(status, Status::Ok | Status::Stale) {
                        match &put.record {
                            PutRecord::Media(replica) => {
                                media_written.push(dyapp_identity::sha256(&replica.data).to_vec())
                            }
                            PutRecord::Sharded(replica, _) => {
                                media_written.push(replica.manifest.as_ref().unwrap().hash.clone())
                            }
                            _ => {}
                        }
                        if let PutRecord::Profile(record) = &put.record {
                            profiles.push(put.stale.clone().unwrap_or_else(|| record.clone()));
                        }
                    }
                    reply.send(swarm, service, status, put.stale.clone());
                }
            }
        }
        for hash in media_written {
            self.media_repaired.finish(&hash, false);
        }
        for record in profiles {
            self.repair_profile(swarm, service, record, now);
        }
        // Keep background fan-out alive after the ack, but bound its lifetime and all indices.
        self.puts.retain(|id, put| {
            let active = self
                .lookups
                .values()
                .any(|lookup| matches!(lookup, Lookup::Put(p) if p == id))
                || self.mailbox_puts.values().any(|p| p == id)
                || self.profile_puts.values().any(|p| p == id)
                || self.media_puts.values().any(|p| p == id)
                || self.shard_puts.values().any(|(p, _)| p == id)
                || self.lookups.values().any(|l| matches!(l, Lookup::ShardPut(p, _) if p == id))
                || self.media_queue.iter().any(|(_, job)| matches!(job, MediaJob::Put(p) | MediaJob::ShardPut(p, _) if p == id));
            now < put.deadline && (put.reply.is_some() || active)
        });
        for (id, lookup) in &self.lookups {
            if matches!(lookup, Lookup::Put(put) | Lookup::ShardPut(put, _) if !self.puts.contains_key(put))
            {
                if let Some(mut query) = swarm.behaviour_mut().kad.query_mut(id) {
                    query.finish();
                }
            }
        }
        self.mailbox_puts.retain(|_, id| self.puts.contains_key(id));
        self.profile_puts.retain(|_, id| self.puts.contains_key(id));
        self.media_puts.retain(|_, id| self.puts.contains_key(id));
        self.shard_puts
            .retain(|_, (id, _)| self.puts.contains_key(id));
        self.lookups
            .retain(|_, lookup| !matches!(lookup, Lookup::Put(id) | Lookup::ShardPut(id, _) if !self.puts.contains_key(id)));
        self.finish_reads(swarm, service, now);
        self.finish_media_repairs(swarm, service, now);
        self.drain_media(swarm, service);
    }

    fn shard_holder(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: u64,
        index: usize,
        to: PeerId,
    ) {
        let Some(put) = self.puts.get_mut(&id) else {
            return;
        };
        let PutRecord::Sharded(replica, shards) = &put.record else {
            return;
        };
        if !put.shard_sent.insert((index, to)) {
            return;
        }
        if to == *swarm.local_peer_id() {
            let request = proto::MediaShardPut {
                replica: Some(replica.clone()),
                index: index as u32,
                data: shards[index].clone(),
            };
            match service.store_shard(&request) {
                Ok(response) if response.status == i32::from(Status::Ok) => {
                    put.shard_stored.insert(index);
                }
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "local shard failed"),
            }
        } else if self.trust.trusted(&to) {
            self.media_queue
                .push_back((to, MediaJob::ShardPut(id, index)));
        }
    }

    /// The remote media codec admits two streams per connection: queue excess work locally.
    fn drain_media(&mut self, swarm: &mut Swarm<Behaviour>, service: &Service) {
        let count = self.media_queue.len();
        for _ in 0..count {
            let (to, job) = self.media_queue.pop_front().unwrap();
            let live = match job {
                MediaJob::Put(id) | MediaJob::ShardPut(id, _) => self.puts.contains_key(&id),
                MediaJob::ShardGet(id, _) => self.media_reads.contains_key(&id),
                MediaJob::Repair(id, job) => self
                    .media_repairs
                    .get(&id)
                    .is_some_and(|(r, _, _)| r.has_job(job)),
            };
            if !live {
                continue;
            }
            if self.media_outgoing.len() >= 16
                || self
                    .media_outgoing
                    .values()
                    .filter(|peer| **peer == to)
                    .count()
                    >= 2
            {
                self.media_queue.push_back((to, job));
                continue;
            }
            if let MediaJob::Repair(id, job_id) = job {
                let Some((request, reserve)) = self
                    .media_repairs
                    .get(&id)
                    .and_then(|(r, _, _)| r.request(job_id))
                else {
                    continue;
                };
                let local = to == *swarm.local_peer_id();
                let allowed = local
                    || service
                        .traffic
                        .try_add(request.encoded_len() as u64 + reserve, SHED_MEDIA);
                if local || !allowed {
                    let response = if local {
                        match request.request.as_ref().unwrap() {
                            proto::media_request::Request::Have(have) => {
                                service.media_inventory(&have.items).ok()
                            }
                            proto::media_request::Request::ReplicaPut(replica) => {
                                service.store_media(replica).ok()
                            }
                            proto::media_request::Request::ShardPut(put) => {
                                service.store_shard(put).ok()
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some((repair, _, _)) = self.media_repairs.get_mut(&id) {
                        repair.response(job_id, response);
                    }
                } else {
                    let request_id = swarm.behaviour_mut().media.send_request(&to, request);
                    self.media_outgoing.insert(request_id, to);
                    self.media_repair_requests
                        .insert(request_id, (id, job_id, reserve));
                }
                continue;
            }
            let request = match job {
                MediaJob::Repair(_, _) => unreachable!(),
                MediaJob::Put(id) => {
                    let Some(put) = self.puts.get(&id) else {
                        continue;
                    };
                    let replica = match &put.record {
                        PutRecord::Media(r) | PutRecord::Sharded(r, _) => r.clone(),
                        _ => continue,
                    };
                    proto::media_request::Request::ReplicaPut(replica)
                }
                MediaJob::ShardPut(id, index) => {
                    let Some(put) = self.puts.get(&id) else {
                        continue;
                    };
                    let PutRecord::Sharded(replica, shards) = &put.record else {
                        continue;
                    };
                    proto::media_request::Request::ShardPut(proto::MediaShardPut {
                        replica: Some(replica.clone()),
                        index: index as u32,
                        data: shards[index].clone(),
                    })
                }
                MediaJob::ShardGet(id, index) => {
                    let Some(read) = self.media_reads.get(&id) else {
                        continue;
                    };
                    if read.shards.contains_key(&index) {
                        continue;
                    }
                    proto::media_request::Request::ShardGet(proto::MediaShardGet {
                        hash: read.manifest.hash.clone(),
                        index: index as u32,
                    })
                }
            };
            let prepaid = match &request {
                proto::media_request::Request::ReplicaPut(r) => r.data.len(),
                proto::media_request::Request::ShardPut(p) => p.data.len(),
                _ => 0,
            };
            let request = proto::MediaRequest {
                request: Some(request),
            };
            service
                .traffic
                .add((request.encoded_len() - prepaid) as u64);
            let request_id = swarm.behaviour_mut().media.send_request(&to, request);
            self.media_outgoing.insert(request_id, to);
            match job {
                MediaJob::Repair(_, _) => unreachable!(),
                MediaJob::Put(id) => {
                    self.media_puts.insert(request_id, id);
                }
                MediaJob::ShardPut(id, index) => {
                    self.shard_puts.insert(request_id, (id, index));
                }
                MediaJob::ShardGet(id, index) => {
                    self.shard_gets.insert(request_id, (id, index));
                }
            }
        }
    }

    fn start_read(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        manifest: proto::MediaManifest,
        reply: libp2p::request_response::ResponseChannel<proto::MediaResponse>,
    ) {
        let id = self.next_put;
        self.next_put = self.next_put.wrapping_add(1);
        let mut read = MediaRead {
            manifest,
            reply,
            deadline: Instant::now() + PUT_TIMEOUT,
            shards: HashMap::new(),
        };
        for index in 0..read.manifest.shard_hashes.len() {
            match service
                .media
                .as_ref()
                .unwrap()
                .shard(&read.manifest.hash, index)
            {
                Ok(Some(data)) if read.manifest.accepts(index, &data) => {
                    read.shards.insert(index, data);
                }
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "local shard read failed"),
            }
        }
        let hash = read.manifest.hash.clone();
        let count = read.manifest.shard_hashes.len();
        self.media_reads.insert(id, read);
        if self.media_reads[&id].shards.len()
            < self.media_reads[&id].manifest.data_shards().unwrap()
        {
            for index in 0..count {
                if self.media_reads[&id].shards.contains_key(&index) {
                    continue;
                }
                let query = swarm
                    .behaviour_mut()
                    .kad
                    .get_closest_peers(replica_key(&hash, index as u8));
                self.lookups.insert(query, Lookup::ShardGet(id, index));
            }
        }
        self.finish_puts(swarm, service, Instant::now());
    }

    fn finish_reads(&mut self, swarm: &mut Swarm<Behaviour>, service: &Service, now: Instant) {
        let done: Vec<_> = self
            .media_reads
            .iter()
            .filter_map(|(id, read)| {
                (now >= read.deadline || read.shards.len() >= read.manifest.data_shards().unwrap())
                    .then_some(*id)
            })
            .collect();
        for id in done {
            let read = self.media_reads.remove(&id).unwrap();
            let shards: Vec<_> = read.shards.into_iter().collect();
            let response = if shards.len() >= read.manifest.data_shards().unwrap() {
                service
                    .assemble_media(&read.manifest, &shards)
                    .unwrap_or_else(|error| {
                        tracing::error!(%error, "media assembly failed");
                        proto::MediaResponse {
                            status: Status::NotFound.into(),
                            ..Default::default()
                        }
                    })
            } else {
                proto::MediaResponse {
                    status: Status::NotFound.into(),
                    ..Default::default()
                }
            };
            service
                .traffic
                .add((response.encoded_len() - response.data.len()) as u64);
            let _ = swarm
                .behaviour_mut()
                .media
                .send_response(read.reply, response);
        }
        for (id, lookup) in &self.lookups {
            if matches!(lookup, Lookup::ShardGet(read, _) if !self.media_reads.contains_key(read)) {
                if let Some(mut query) = swarm.behaviour_mut().kad.query_mut(id) {
                    query.finish();
                }
            }
        }
        self.lookups.retain(
            |_, l| !matches!(l, Lookup::ShardGet(id, _) if !self.media_reads.contains_key(id)),
        );
    }
}

/// Envelope bytes in one `replica_put`; a larger gap fills over the next repairs.
const REPAIR_BYTES: usize = 1024 * 1024;

/// Sends `to` the held envelopes of `mailbox` that `wanted` picks, in one `replica_put`.
fn send_envelopes(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    to: PeerId,
    mailbox: &[u8],
    wanted: impl Fn(&Vec<u8>) -> bool,
) {
    let held = match service.held(mailbox) {
        Ok(held) => held,
        Err(error) => return tracing::error!(%error, "mailbox repair failed"),
    };
    let mut bytes = 0;
    let envelopes: Vec<SignedRecord> = held
        .into_iter()
        .filter(|(id, _)| wanted(id))
        .map(|(_, record)| record)
        .take_while(|record| {
            bytes += record.encoded_len();
            bytes <= REPAIR_BYTES
        })
        .collect();
    if !envelopes.is_empty() {
        service
            .traffic
            .add(envelopes.iter().map(|r| r.encoded_len() as u64).sum());
        let request = mailbox_request::Request::ReplicaPut(proto::Envelopes { envelopes });
        let request = MailboxRequest {
            request: Some(request),
        };
        swarm.behaviour_mut().mailbox.send_request(&to, request);
    }
}

/// What follows a successful mailbox request: a put is pushed to the mailbox's watchers; a
/// watching fetch is remembered and starts a repair; a device's ack is forwarded to the nodes
/// closest to the replica keys (its replies are ignored: the envelopes expire anyway); an
/// inventory's sender gets the envelopes it lacks if it is near a replica key.
/// ponytail: acks and inventories go to the closest node of each replica key only, where clients
/// write; a replica written elsewhere because that node was full is not repaired and keeps an
/// acked envelope until it expires.
fn after_mailbox(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    watchers: &mut Watchers,
    replicas: &mut Replicas,
    (peer, connection): (PeerId, ConnectionId),
    request: Option<mailbox_request::Request>,
) {
    let owner = |record: &SignedRecord| {
        <[u8; 32]>::try_from(record.public_key.as_slice())
            .map(|key| dyapp_identity::key_hash(&key).to_vec())
    };
    let mut lookup = |swarm: &mut Swarm<Behaviour>, mailbox: &[u8], then: &dyn Fn() -> Lookup| {
        for i in 0..REPLICAS {
            let key = replica_key(mailbox, i);
            let query = swarm.behaviour_mut().kad.get_closest_peers(key);
            replicas.lookups.insert(query, then());
        }
    };
    let repaired = &mut replicas.repaired;
    match request {
        Some(mailbox_request::Request::Put(record)) => push(swarm, watchers, vec![record]),
        // ponytail: a batch with one failed envelope is not pushed; the device fetches the rest.
        Some(mailbox_request::Request::ReplicaPut(batch)) => push(swarm, watchers, batch.envelopes),
        Some(mailbox_request::Request::Fetch(record)) => {
            let watch = proto::Fetch::decode(record.payload.as_slice()).is_ok_and(|f| f.watch);
            let (true, Ok(mailbox)) = (watch, owner(&record)) else {
                return;
            };
            let list = watchers.entry(mailbox.clone()).or_default();
            if !list.contains(&(peer, connection)) {
                list.push((peer, connection));
            }
            if service.traffic.second() < SHED_MEDIA && repaired.insert(mailbox.clone()) {
                lookup(swarm, &mailbox, &|| Lookup::Repair(mailbox.clone()));
            }
        }
        Some(mailbox_request::Request::Ack(record)) => {
            let Ok(mailbox) = owner(&record) else { return };
            lookup(swarm, &mailbox, &|| Lookup::Ack(record.clone()));
        }
        Some(mailbox_request::Request::Inventory(inventory)) => {
            // An arbitrary peer may send an inventory; only one this node's routing table
            // places near a replica key gets envelopes back.
            let near = (0..REPLICAS).any(|i| {
                let key = kad::KBucketKey::new(replica_key(&inventory.mailbox, i));
                let kad = &mut swarm.behaviour_mut().kad;
                let mut closest = kad.get_closest_local_peers(&key).take(REPLICAS.into());
                closest.any(|p| *p.preimage() == peer)
            });
            if near && replicas.trust.trusted(&peer) {
                let listed: HashSet<Vec<u8>> = inventory.ids.into_iter().collect();
                send_envelopes(swarm, service, peer, &inventory.mailbox, |id| {
                    !listed.contains(id)
                });
            }
        }
        _ => {}
    }
}

/// Pushes stored envelopes to the watchers of their mailboxes.
fn push(swarm: &mut Swarm<Behaviour>, watchers: &Watchers, envelopes: Vec<SignedRecord>) {
    for record in envelopes {
        let Ok(envelope) = proto::Envelope::decode(record.payload.as_slice()) else {
            continue;
        };
        for (watcher, _) in watchers.get(&envelope.mailbox).into_iter().flatten() {
            let push = proto::MailboxPush {
                envelopes: vec![record.clone()],
            };
            swarm.behaviour_mut().push.send_request(watcher, push);
        }
    }
}

/// A finished replica-key lookup: forwards the ack, or sends the inventory unless this node is
/// itself the closest to the key.
fn after_lookup(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    replicas: &mut Replicas,
    lookup: Lookup,
    key: Vec<u8>,
    node: kad::PeerInfo,
) {
    let (request, repair) = match lookup {
        Lookup::MediaRepair(_, _, _)
        | Lookup::Put(_)
        | Lookup::ShardPut(_, _)
        | Lookup::ShardGet(_, _)
        | Lookup::ProfileRepair(_) => unreachable!("lookups use all returned peers"),
        Lookup::Ack(ack) => (mailbox_request::Request::ReplicaAck(ack), None),
        Lookup::Repair(mailbox) => {
            let target = kad::KBucketKey::new(key);
            let me = kad::KBucketKey::from(*swarm.local_peer_id());
            if me.distance(&target) < kad::KBucketKey::from(node.peer_id).distance(&target) {
                return;
            }
            let ids = match service.held(&mailbox) {
                Ok(held) => held.into_keys().collect(),
                Err(error) => return tracing::error!(%error, "mailbox repair failed"),
            };
            let inventory = proto::Inventory {
                mailbox: mailbox.clone(),
                ids,
            };
            (
                mailbox_request::Request::Inventory(inventory),
                Some(mailbox),
            )
        }
    };
    for address in node.addrs {
        swarm.add_peer_address(node.peer_id, address);
    }
    let request = MailboxRequest {
        request: Some(request),
    };
    service.traffic.add(request.encoded_len() as u64);
    let id = swarm
        .behaviour_mut()
        .mailbox
        .send_request(&node.peer_id, request);
    if let Some(mailbox) = repair {
        replicas.inventories.insert(id, mailbox);
    }
}

/// Serves requests on `swarm` until the task is dropped. The deny list is checked for changes
/// every 10 seconds and on SIGHUP; an old `<storage.dir>/deny` file is imported once at start.
pub async fn run(mut swarm: Swarm<Behaviour>, mut service: Service) {
    use tokio::signal::unix::{signal, SignalKind};
    let mut hangup = match signal(SignalKind::hangup()) {
        Ok(hangup) => hangup,
        Err(error) => return tracing::error!(%error, "SIGHUP handler not installed"),
    };
    let storage = &service.config.storage;
    let mut anchors = cached_peers(&storage.anchors_path());
    let deny = match DenyStore::open(&storage.deny_path()) {
        Ok(deny) => deny,
        Err(error) => return tracing::error!(%error, "deny list not opened"),
    };
    match deny.import(&storage.dir.join("deny"), &service.config.limits) {
        Ok(0) => {}
        Ok(imported) => tracing::info!(imported, "deny file moved into deny.db"),
        Err(error) => tracing::error!(%error, "deny file not imported"),
    }
    let mut deny_version = None;
    reload_deny(&mut service, &deny, &mut deny_version);
    let mut deny_check = tokio::time::interval(Duration::from_secs(10));
    // The mailbox challenge issued on each open connection.
    let mut nonces: HashMap<ConnectionId, [u8; 32]> = HashMap::new();
    // The IP group of each open connection.
    let mut groups: HashMap<ConnectionId, String> = HashMap::new();
    // The remote address of each connection this node dialled.
    let mut dialed: HashMap<ConnectionId, Multiaddr> = HashMap::new();
    let mut watchers = Watchers::new();
    let delay = u64::from(service.config.network.storage_trust_minutes) * 60;
    let mut replicas = Replicas::default();
    replicas.trust.delay = Duration::from_secs(delay);
    // Incoming connections refused by the connection limits since the last maintenance run.
    let mut refused = 0u64;
    // A turn node announces its relay once it has a routing peer; kad republishes it every 12 h.
    // ponytail: every relay is a provider of one key, and a node keeps at most 20 providers per
    // key; shard the key (by region or prefix) when the network has more relays than that.
    let mut turn_unannounced = service.config.roles.contains(&Role::Turn);
    let mut turn_query = None;
    // Requests answered and failed with a node error since the last maintenance run.
    let (mut answered, mut failed) = (0u64, 0u64);
    let mut put_check = tokio::time::interval(Duration::from_millis(100));
    let mut cleanup = tokio::time::interval(Duration::from_secs(3600));
    let maintenance = service.config.maintenance.clone();
    let mut maintain = tokio::time::interval(Duration::from_secs(
        u64::from(maintenance.interval_minutes) * 60,
    ));
    // Seeds are dialled every maintenance run (the first at start) and, while the routing table
    // is empty, every 5 minutes. `NodeConfig::validate` has checked that they parse.
    let seeds: Vec<Multiaddr> = service
        .config
        .seeds
        .iter()
        .filter_map(|s| crate::config::seed(s).ok())
        .flatten()
        .collect();
    let every = Duration::from_secs(300);
    let mut rejoin = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
            dyapp_p2p_net::route(&mut swarm, &event);
            match event {
                SwarmEvent::NewListenAddr { address, .. } => tracing::info!(%address, "listening"),
                SwarmEvent::ExternalAddrConfirmed { address } => {
                    tracing::info!(%address, "external address confirmed")
                }
                // At max_memory_mb a request is dropped unanswered: its stream is reset and the
                // client tries another node.
                SwarmEvent::Behaviour(
                    BehaviourEvent::Node(Event::Message { message: Message::Request { .. }, .. })
                    | BehaviourEvent::Profile(Event::Message {
                        message: Message::Request { .. },
                        ..
                    })
                    | BehaviourEvent::Media(Event::Message {
                        message: Message::Request { .. },
                        ..
                    })
                    | BehaviourEvent::Mailbox(Event::Message {
                        message: Message::Request { .. },
                        ..
                    }),
                ) if service.overloaded() => {}
                // ponytail: SQLite calls block the swarm loop; move them to spawn_blocking when
                // load makes request latency visible.
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let response = service.node(&peer(id, &groups, connection_id), request);
                    answered += 1;
                    service.traffic.add(response.encoded_len() as u64);
                    let _ = swarm.behaviour_mut().node.send_response(channel, response);
                }
                // The node asks other nodes only for deny lists.
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                    peer: id,
                    message: Message::Response { response, .. },
                    ..
                })) => {
                    if let Some(record) = response.deny_list {
                        keep_deny_list(&deny, &id, &record);
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    if let Some(proto::profile_request::Request::Publish(record)) = &request.request {
                        answered += 1;
                        match service.prepare_profile(&peer(id, &groups, connection_id), record.clone()) {
                            Ok(response) if response.status == i32::from(Status::Ok) => {
                                replicas.start_put(&mut swarm, &service, &watchers, PutRecord::Profile(record.clone()), PutReply::Profile(channel));
                            }
                            Ok(response) => {
                                service.traffic.add(response.encoded_len() as u64);
                                let _ = swarm.behaviour_mut().profile.send_response(channel, response);
                            }
                            Err(error) => {
                                answered -= 1;
                                failed += 1;
                                tracing::error!(%error, "profile admission failed");
                            }
                        }
                        drop_banned(&mut swarm, &service, id);
                        continue;
                    }
                    let heartbeat = match &request.request {
                        Some(proto::profile_request::Request::Heartbeat(record)) => Some(record.public_key.clone()),
                        _ => None,
                    };
                    match service.profile(&peer(id, &groups, connection_id), request) {
                        Ok(response) => {
                            if response.status == i32::from(Status::Ok) {
                                if let Some(key) = heartbeat {
                                    let owner = dyapp_identity::key_hash(&key.as_slice().try_into().unwrap());
                                    match service.store.get_profile(&owner.iter().map(|byte| format!("{byte:02x}")).collect::<String>()) {
                                        Ok(Some(record)) => replicas.repair_profile(&mut swarm, &service, record, Instant::now()),
                                        Ok(None) => {},
                                        Err(error) => tracing::error!(%error, "heartbeat repair failed"),
                                    }
                                }
                            }
                            answered += 1;
                            service.traffic.add(response.encoded_len() as u64);
                            let _ = swarm.behaviour_mut().profile.send_response(channel, response);
                        }
                        // Dropping the channel fails the request; the client tries another node.
                        Err(error) => {
                            failed += 1;
                            tracing::error!(%error, "profile request failed")
                        }
                    }
                    drop_banned(&mut swarm, &service, id);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let from = peer(id, &groups, connection_id);
                    if let Some(proto::media_request::Request::Put(put)) = &request.request {
                        let result = if replicas.at_capacity() { Ok(Err(Status::Full)) }
                            else { service.prepare_media(&from, put) };
                        match result {
                            Ok(Ok(replica)) => {
                                answered += 1;
                                replicas.start_put(&mut swarm, &service, &watchers, PutRecord::Media(replica), PutReply::Media(channel));
                            }
                            Ok(Err(status)) => {
                                answered += 1;
                                PutReply::Media(channel).send(&mut swarm, &service, status, None);
                            }
                            Err(error) => { failed += 1; tracing::error!(%error, "media admission failed"); }
                        }
                        drop_banned(&mut swarm, &service, id);
                        continue;
                    }
                    let keep = match &request.request {
                        Some(proto::media_request::Request::Keep(record)) => Some(record.clone()),
                        _ => None,
                    };
                    let have = match &request.request {
                        Some(proto::media_request::Request::Have(have)) => Some(have.items.clone()),
                        _ => None,
                    };
                    let get = match &request.request {
                        Some(proto::media_request::Request::Get(get)) => Some(get.hash.clone()),
                        _ => None,
                    };
                    match service.media(&from, request) {
                        Ok(mut response) => {
                            answered += 1;
                            if response.status == i32::from(Status::Ok) {
                                if let Some(items) = have {
                                    response = replicas.media_inventory(&mut swarm, &service, id, &items).unwrap_or_else(|error| {
                                        tracing::error!(%error, "media inventory failed");
                                        proto::MediaResponse { status: Status::NotFound.into(), ..Default::default() }
                                    });
                                    let _ = swarm.behaviour_mut().media.send_response(channel, response);
                                    continue;
                                }
                                if let Some(record) = keep {
                                    replicas.repair_media(&mut swarm, &service, record, channel, response, Instant::now());
                                    continue;
                                }
                            }
                            if response.status == i32::from(Status::NotFound) {
                                if let (Some(hash), Some(media)) = (&get, &service.media) {
                                    if let Ok(Some(manifest)) = media.manifest(hash) {
                                        if replicas.media_reads.len() >= 4 || !service.prepare_media_read(&from, &manifest) {
                                            PutReply::Media(channel).send(&mut swarm, &service, Status::RateLimited, None);
                                        } else { replicas.start_read(&mut swarm, &service, manifest, channel); }
                                        continue;
                                    }
                                }
                            }
                            // A get's payload was reserved by the service before returning it.
                            service.traffic.add((response.encoded_len() - response.data.len()) as u64);
                            let _ = swarm.behaviour_mut().media.send_response(channel, response);
                        }
                        Err(error) => {
                            failed += 1;
                            tracing::error!(%error, "media request failed")
                        }
                    }
                    drop_banned(&mut swarm, &service, id);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    if let Some(mailbox_request::Request::Put(record)) = &request.request {
                        answered += 1;
                        match service.prepare_mailbox(&peer(id, &groups, connection_id), record.clone()) {
                            Ok(response) if response.status == i32::from(Status::Ok) => {
                                replicas.start_put(&mut swarm, &service, &watchers, PutRecord::Mailbox(record.clone()), PutReply::Mailbox(channel));
                            }
                            Ok(response) => {
                                service.traffic.add(response.encoded_len() as u64);
                                let _ = swarm.behaviour_mut().mailbox.send_response(channel, response);
                            }
                            Err(error) => {
                                answered -= 1;
                                failed += 1;
                                tracing::error!(%error, "mailbox admission failed");
                            }
                        }
                        drop_banned(&mut swarm, &service, id);
                        continue;
                    }
                    let mut nonce = nonces.remove(&connection_id);
                    let from = peer(id, &groups, connection_id);
                    let kind = request.request.clone();
                    let signed = matches!(
                        kind,
                        Some(mailbox_request::Request::ReplicaPut(_))
                            | Some(mailbox_request::Request::ReplicaAck(_))
                    );
                    let result = service.mailbox(&from, &mut nonce, request);
                    if let Some(nonce) = nonce {
                        nonces.insert(connection_id, nonce);
                    }
                    match result {
                        Ok(response) => {
                            let ok = response.status == i32::from(Status::Ok);
                            answered += 1;
                    service.traffic.add(response.encoded_len() as u64);
                            let _ = swarm.behaviour_mut().mailbox.send_response(channel, response);
                            if ok && signed {
                                replicas.trust.add(id, 1);
                            }
                            if ok {
                                let at = (id, connection_id);
                                let (w, r) = (&mut watchers, &mut replicas);
                                after_mailbox(&mut swarm, &service, w, r, at, kind);
                            }
                        }
                        Err(error) => {
                            failed += 1;
                            tracing::error!(%error, "mailbox request failed")
                        }
                    }
                    drop_banned(&mut swarm, &service, id);
                }
                SwarmEvent::ConnectionEstablished { peer_id, connection_id, endpoint, .. } => {
                    let group = ip_group(endpoint.get_remote_address(), &service.config.limits);
                    groups.insert(connection_id, group);
                    replicas.trust.seen.entry(peer_id).or_insert_with(Instant::now);
                    if endpoint.is_dialer() {
                        dialed.insert(connection_id, endpoint.get_remote_address().clone());
                    }
                    drop_banned(&mut swarm, &service, peer_id);
                }
                SwarmEvent::ConnectionClosed { connection_id, .. } => {
                    nonces.remove(&connection_id);
                    groups.remove(&connection_id);
                    dialed.remove(&connection_id);
                    // ponytail: a scan of all watchers per closed connection; index them by
                    // connection if many devices watch at once.
                    watchers.retain(|_, list| {
                        list.retain(|(_, c)| *c != connection_id);
                        !list.is_empty()
                    });
                }
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::StartProviding(result),
                    ..
                })) if turn_query == Some(id) => {
                    turn_query = None;
                    match result {
                        Ok(_) => {
                            turn_unannounced = false;
                            tracing::info!("TURN relay announced");
                        }
                        Err(error) => tracing::warn!(%error, "TURN relay not announced"),
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::GetClosestPeers(result),
                    ..
                })) => {
                    let lookup = replicas.lookups.remove(&id);
                    if result.is_err() {
                        if let Some(Lookup::MediaRepair(repair, blob, index)) = &lookup {
                            if let Some((r, _, _)) = replicas.media_repairs.get_mut(repair) { r.holder(*blob, *index, None); }
                            replicas.finish_puts(&mut swarm, &service, Instant::now());
                        }
                    }
                    if let (Some(lookup), Ok(ok)) = (lookup, result) {
                        // A peer without the node-ID proof of work never holds a replica.
                        let bits = dyapp_p2p_net::id_pow_bits();
                        if matches!(lookup, Lookup::MediaRepair(_, _, _) | Lookup::Put(_) | Lookup::ShardPut(_, _) | Lookup::ShardGet(_, _) | Lookup::ProfileRepair(_)) {
                            let nodes: Vec<_> = ok.peers.into_iter().filter(|node| dyapp_p2p_net::id_has_pow(&node.peer_id, bits)).collect();
                            let to = holder(*swarm.local_peer_id(), &ok.key, &nodes);
                            for node in nodes {
                                for address in node.addrs { swarm.add_peer_address(node.peer_id, address); }
                            }
                            match lookup {
                                Lookup::MediaRepair(repair, blob, index) => {
                                    let trusted = to == *swarm.local_peer_id() || replicas.trust.trusted(&to);
                                    if let Some((r, _, _)) = replicas.media_repairs.get_mut(&repair) {
                                        r.holder(blob, index, trusted.then_some(to));
                                    }
                                    replicas.finish_puts(&mut swarm, &service, Instant::now());
                                }
                                Lookup::Put(put) => {
                                    replicas.put_holder(&mut swarm, &service, &watchers, put, to);
                                    replicas.finish_puts(&mut swarm, &service, Instant::now());
                                },
                                Lookup::ShardPut(put, index) => {
                                    replicas.shard_holder(&mut swarm, &service, put, index, to);
                                    replicas.finish_puts(&mut swarm, &service, Instant::now());
                                }
                                Lookup::ShardGet(read, index) => {
                                    if to != *swarm.local_peer_id() && replicas.trust.trusted(&to) {
                                        replicas.media_queue.push_back((to, MediaJob::ShardGet(read, index)));
                                        replicas.drain_media(&mut swarm, &service);
                                    }
                                }
                                Lookup::ProfileRepair(record) => replicas.profile_inventory(&mut swarm, &service, to, record),
                                _ => unreachable!(),
                            }
                            continue;
                        }
                        let mut peers = ok.peers.into_iter();
                        if let Some(node) =
                            peers.find(|p| {
                                dyapp_p2p_net::id_has_pow(&p.peer_id, bits)
                                    && replicas.trust.trusted(&p.peer_id)
                            })
                        {
                            let r = &mut replicas;
                            after_lookup(&mut swarm, &service, r, lookup, ok.key, node);
                        }
                    }
                }
                // The peer's answer to an inventory: it gets what it lacks.
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                    peer: id,
                    message: Message::Response { request_id, response },
                    ..
                })) => {
                    replicas.trust.add(id, 1);
                    if let Some(put) = replicas.mailbox_puts.remove(&request_id) {
                        if let Some(put) = replicas.puts.get_mut(&put) { put.confirm(id, response.status, None); }
                        replicas.finish_puts(&mut swarm, &service, Instant::now());
                    }
                    if let Some(mailbox) = replicas.inventories.remove(&request_id) {
                        let missing: HashSet<Vec<u8>> = response.missing.into_iter().collect();
                        send_envelopes(&mut swarm, &service, id, &mailbox, |envelope| {
                            missing.contains(envelope)
                        });
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::OutboundFailure {
                    peer,
                    request_id,
                    error,
                    ..
                })) => {
                    // An old node lacking the protocol is no failure.
                    if !matches!(error, OutboundFailure::UnsupportedProtocols) {
                        replicas.trust.add(peer, -1);
                    }
                    replicas.inventories.remove(&request_id);
                    replicas.mailbox_puts.remove(&request_id);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    peer: id,
                    message: Message::Response { request_id, response },
                    ..
                })) => {
                    service.traffic.add(response.encoded_len() as u64);
                    replicas.trust.add(id, 1);
                    if let Some(record) = replicas.profile_inventories.remove(&request_id) {
                        replicas.profile_gap(&mut swarm, &service, id, &record, &response);
                    }
                    if let Some(put) = replicas.profile_puts.remove(&request_id) {
                        if let Some(put) = replicas.puts.get_mut(&put) { put.confirm(id, response.status, response.record); }
                        replicas.finish_puts(&mut swarm, &service, Instant::now());
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::OutboundFailure {
                    peer, request_id, error, ..
                })) => {
                    if !matches!(error, OutboundFailure::UnsupportedProtocols) { replicas.trust.add(peer, -1); }
                    replicas.profile_puts.remove(&request_id);
                    replicas.profile_inventories.remove(&request_id);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                    peer: id, message: Message::Response { request_id, response }, ..
                })) => {
                    replicas.media_outgoing.remove(&request_id);
                    if let Some((repair, job, reserve)) = replicas.media_repair_requests.remove(&request_id) {
                        service.traffic.add((response.encoded_len() as u64).saturating_sub(reserve));
                        if let Some((repair, _, _)) = replicas.media_repairs.get_mut(&repair) { repair.response(job, Some(response)); }
                        replicas.trust.add(id, 1);
                        replicas.finish_puts(&mut swarm, &service, Instant::now());
                        continue;
                    }
                    let prepaid = replicas.shard_gets.contains_key(&request_id);
                    service.traffic.add((response.encoded_len() - if prepaid { response.data.len() } else { 0 }) as u64);
                    replicas.trust.add(id, 1);
                    if let Some((put, index)) = replicas.shard_puts.remove(&request_id) {
                        if response.status == i32::from(Status::Ok) {
                            if let Some(put) = replicas.puts.get_mut(&put) { put.shard_stored.insert(index); }
                        }
                    }
                    if let Some((read, index)) = replicas.shard_gets.remove(&request_id) {
                        if let Some(read) = replicas.media_reads.get_mut(&read) {
                            if response.status == i32::from(Status::Ok) && read.manifest.accepts(index, &response.data) {
                                read.shards.insert(index, response.data.clone());
                            }
                        }
                    }
                    if let Some(put) = replicas.media_puts.remove(&request_id) {
                        if let Some(put) = replicas.puts.get_mut(&put) { put.confirm(id, response.status, None); }
                    }
                    replicas.finish_puts(&mut swarm, &service, Instant::now());
                }
                SwarmEvent::Behaviour(BehaviourEvent::Media(Event::OutboundFailure {
                    peer, request_id, error, ..
                })) => {
                    if !matches!(error, OutboundFailure::UnsupportedProtocols) { replicas.trust.add(peer, -1); }
                    if let Some((repair, job, _)) = replicas.media_repair_requests.remove(&request_id) {
                        if let Some((repair, _, _)) = replicas.media_repairs.get_mut(&repair) { repair.response(job, None); }
                    }
                    replicas.media_puts.remove(&request_id);
                    replicas.shard_puts.remove(&request_id);
                    replicas.shard_gets.remove(&request_id);
                    replicas.media_outgoing.remove(&request_id);
                    replicas.finish_puts(&mut swarm, &service, Instant::now());
                }
                // Kademlia learns a dialer's address only from identify: without this a node
                // never routes to peers that joined through it.
                // ponytail: claimed addresses pass the IP-group limits but are not verified by a
                // dial; a peer can claim another group's address.
                SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                    connection_id,
                    peer_id,
                    info,
                })) if info.protocols.contains(&KAD_PROTOCOL) => {
                    // A routable peer this node dialled answered: it becomes an anchor.
                    let bits = dyapp_p2p_net::id_pow_bits();
                    if let Some(address) = dialed.get(&connection_id) {
                        if dyapp_p2p_net::id_has_pow(&peer_id, bits) {
                            remember_anchor(&mut anchors, peer_id, address);
                        }
                    }
                    for address in info.listen_addrs {
                        dyapp_p2p_net::add_peer(&mut swarm, peer_id, address);
                    }
                    // Peers enter the routing table here, not through kad's own inserts.
                    if turn_unannounced && turn_query.is_none() {
                        let key = kad::RecordKey::new(&dyapp_p2p_net::TURN_KEY);
                        turn_query = swarm.behaviour_mut().kad.start_providing(key).ok();
                    }
                }
                SwarmEvent::IncomingConnectionError { error: ListenError::Denied { .. }, .. } => {
                    refused += 1;
                }
                _ => {}
            }
            },
            _ = put_check.tick() => replicas.finish_puts(&mut swarm, &service, Instant::now()),
            _ = deny_check.tick() => reload_deny(&mut service, &deny, &mut deny_version),
            _ = hangup.recv() => {
                deny_version = None;
                reload_deny(&mut service, &deny, &mut deny_version);
            }
            _ = rejoin.tick() => {
                if dyapp_p2p_net::known_peers(&mut swarm).is_empty() {
                    dyapp_p2p_net::join(&mut swarm, &seeds, &[]);
                }
            }
            _ = cleanup.tick() => {
                replicas.repaired.clear();
                let now = chrono::Utc::now().timestamp();
                match service.store.cleanup_expired(now) {
                    Ok(removed) => tracing::info!(removed, "expired messages removed"),
                    Err(error) => tracing::error!(%error, "message cleanup failed"),
                }
                let ttl = i64::from(service.config.limits.profile_ttl_days) * 86_400;
                match service.store.expire_profiles(now - ttl) {
                    Ok(removed) => tracing::info!(removed, "profiles of inactive owners removed"),
                    Err(error) => tracing::error!(%error, "profile cleanup failed"),
                }
                if let Some(media) = &service.media {
                    match media.expire(now.unsigned_abs()) {
                        Ok(removed) => tracing::info!(removed, "expired attachments removed"),
                        Err(error) => tracing::error!(%error, "attachment cleanup failed"),
                    }
                    match media.expire_owners(now - ttl) {
                        Ok(removed) => tracing::info!(removed, "media keeps of inactive owners removed"),
                        Err(error) => tracing::error!(%error, "media owner cleanup failed"),
                    }
                }
            }
            _ = maintain.tick() => {
                match service.store.maintain(maintenance.vacuum_pages) {
                    Ok([profiles, messages]) => {
                        tracing::info!(profiles, messages, "store maintenance done, free pages left")
                    }
                    Err(error) => tracing::error!(%error, "store maintenance failed"),
                }
                service.report();
                service.rebuild_budget.report();
                // ponytail: saved hourly, not on shutdown; a crash loses at most an hour of churn.
                let peers = dyapp_p2p_net::known_peers(&mut swarm);
                let storage = &service.config.storage;
                if !peers.is_empty() {
                    if let Err(error) = save_peers(&storage.peers_path(), &peers) {
                        tracing::error!(%error, "peer cache not saved");
                    }
                }
                if !anchors.is_empty() {
                    if let Err(error) = save_peers(&storage.anchors_path(), &anchors) {
                        tracing::error!(%error, "anchors not saved");
                    }
                }
                let routed: HashSet<PeerId> = peers
                    .iter()
                    .filter_map(|a| match a.iter().last() {
                        Some(Protocol::P2p(peer)) => Some(peer),
                        _ => None,
                    })
                    .collect();
                replicas
                    .trust
                    .retain(|peer| routed.contains(peer) || swarm.is_connected(peer));
                if service.config.network.accept_deny_lists {
                    let request = proto::NodeRequest {
                        request: Some(proto::node_request::Request::DenyList(
                            proto::DenyListRequest {},
                        )),
                    };
                    for peer in &routed {
                        if swarm.is_connected(peer) {
                            swarm.behaviour_mut().node.send_request(peer, request);
                        }
                    }
                }
                if refused > 0 {
                    tracing::warn!(refused, "connections refused by the connection or memory limits");
                    refused = 0;
                }
                status(&service, &swarm, peers.len(), answered, failed, replicas.media_holders.1);
                (answered, failed) = (0, 0);
                dyapp_p2p_net::join(&mut swarm, &seeds, &[]);
            }
        }
    }
}

/// The hourly status line: no peer or key identifiers, counts and sizes only.
fn status(
    service: &Service,
    swarm: &Swarm<Behaviour>,
    known: usize,
    answered: u64,
    failed: u64,
    media_replica_holders: usize,
) {
    let bytes = |usage: crate::Result<(u64, u64)>| usage.map_or(0, |(used, _)| used);
    let media = service
        .media
        .as_ref()
        .map_or(0, |media| bytes(media.usage()));
    tracing::info!(
        connected = swarm.connected_peers().count(),
        known,
        answered,
        failed,
        profiles_bytes = bytes(service.store.profiles_usage()),
        messages_bytes = bytes(service.store.messages_usage()),
        media_bytes = media,
        media_replica_holders,
        "node status"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn media_inventory_requires_near_trusted_peers_and_budget() {
        let dir = format!("/tmp/ai/test-media-inventory-{}", uuid::Uuid::new_v4());
        let mut config = crate::config::NodeConfig::default();
        config.storage.dir = dir.clone().into();
        config.limits.bytes_per_second = 10_000;
        let mut service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
        service.media = Some(crate::media::MediaStore::open(Path::new(&dir)).unwrap());
        let mut swarm = swarm(
            Keypair::generate_ed25519(),
            &service.config.limits,
            &service.config.roles,
        )
        .unwrap();
        let near = PeerId::random();
        let far = PeerId::random();
        swarm
            .behaviour_mut()
            .kad
            .add_address(&near, "/ip4/127.0.0.1/tcp/1".parse().unwrap());
        let items = vec![proto::MediaPart {
            hash: vec![1; 32],
            index: None,
        }];
        let mut replicas = Replicas::default();
        let reply = replicas
            .media_inventory(&mut swarm, &service, far, &items)
            .unwrap()
            .inventory
            .unwrap();
        assert!(
            reply.present.is_empty()
                && reply.missing.is_empty()
                && reply.manifests.is_empty()
                && reply.whole.is_empty()
        );
        replicas.trust.score.insert(near, -1);
        assert!(replicas
            .media_inventory(&mut swarm, &service, near, &items)
            .unwrap()
            .inventory
            .unwrap()
            .missing
            .is_empty());
        replicas.trust.score.insert(near, 0);
        assert_eq!(
            replicas
                .media_inventory(&mut swarm, &service, near, &items)
                .unwrap()
                .inventory
                .unwrap()
                .missing,
            items
        );
        service.traffic.add(10_000);
        assert_eq!(
            replicas
                .media_inventory(&mut swarm, &service, near, &items)
                .unwrap()
                .status,
            i32::from(Status::RateLimited)
        );
    }

    #[tokio::test]
    async fn profile_repair_only_fills_gaps_once_an_hour_within_budget() {
        let dir = format!("/tmp/ai/test-profile-repair-{}", uuid::Uuid::new_v4());
        let mut config = crate::config::NodeConfig::default();
        config.storage.dir = dir.clone().into();
        config.limits.bytes_per_second = 10_000;
        let service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
        let mut swarm = swarm(
            Keypair::generate_ed25519(),
            &service.config.limits,
            &service.config.roles,
        )
        .unwrap();
        let remote = PeerId::random();
        swarm
            .behaviour_mut()
            .kad
            .add_address(&remote, "/ip4/127.0.0.1/tcp/1".parse().unwrap());
        let owner = dyapp_identity::Identity::generate();
        let record = dyapp_profile::Profile {
            version: 2,
            ..Default::default()
        }
        .sign(&owner);
        let key = dyapp_identity::key_hash(&owner.public_key()).to_vec();
        let mut replicas = Replicas::default();
        let now = Instant::now();
        replicas.repair_profile(&mut swarm, &service, record.clone(), now);
        assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
        replicas.repair_profile(
            &mut swarm,
            &service,
            record.clone(),
            now + PROFILE_REPAIR_INTERVAL - Duration::from_nanos(1),
        );
        assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
        replicas.lookups.clear();
        replicas.repair_profile(
            &mut swarm,
            &service,
            record.clone(),
            now + PROFILE_REPAIR_INTERVAL,
        );
        assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
        assert_eq!(
            replicas.profile_repaired[&key].0,
            now + PROFILE_REPAIR_INTERVAL
        );
        for _ in 0..REPLICAS {
            replicas.profile_inventory(&mut swarm, &service, remote, Arc::new(record.clone()));
        }
        assert_eq!(
            replicas.profile_inventories.len(),
            1,
            "co-located replicas are inventoried once"
        );
        for (status, version, sends) in [
            (Status::Ok, 1, true),
            (Status::NotFound, 0, true),
            (Status::Ok, 2, false),
            (Status::Ok, 3, false),
            (Status::Denied, 0, false),
            (Status::RateLimited, 0, false),
        ] {
            let response = proto::ProfileResponse {
                status: status.into(),
                version,
                ..Default::default()
            };
            assert_eq!(
                replicas
                    .profile_gap(&mut swarm, &service, remote, &record, &response)
                    .is_some(),
                sends
            );
        }
        service.traffic.add(10_000);
        let response = proto::ProfileResponse {
            status: Status::NotFound.into(),
            ..Default::default()
        };
        assert!(replicas
            .profile_gap(&mut swarm, &service, remote, &record, &response)
            .is_none());
        let other = PeerId::random();
        replicas.profile_inventory(&mut swarm, &service, other, Arc::new(record));
        assert_eq!(
            replicas.profile_inventories.len(),
            1,
            "repair exceeded its budget"
        );
    }

    #[test]
    fn put_ack_counts_distinct_holders_and_times_out() {
        let record = PutRecord::Mailbox(SignedRecord::default());
        let (a, b) = (PeerId::random(), PeerId::random());
        let mut put = PendingPut::new(record.clone(), None, false);
        put.confirm(a, Status::Ok.into(), None);
        put.confirm(a, Status::Ok.into(), None);
        put.confirm(b, Status::Denied.into(), None);
        assert_eq!(put.result(Instant::now()), None);
        assert_eq!(put.result(put.deadline), Some(Status::Full));
        put.confirm(b, Status::Ok.into(), None);
        assert_eq!(put.result(Instant::now()), Some(Status::Ok));
        assert_eq!(
            put.result(put.deadline),
            Some(Status::Full),
            "late replies beat the deadline"
        );
        let mut isolated = PendingPut::new(record, None, true);
        isolated.confirm(a, Status::Ok.into(), None);
        assert_eq!(isolated.result(Instant::now()), Some(Status::Ok));
    }

    #[tokio::test]
    async fn sharded_writes_require_k_indices_and_queue_two_streams_per_peer() {
        let (manifest, shards) = crate::replication::media_encode(&vec![1; (1 << 20) + 1]).unwrap();
        let replica = proto::MediaReplicaPut {
            manifest: Some(manifest),
            ..Default::default()
        };
        let mut put = PendingPut::new(PutRecord::Sharded(replica, Arc::new(shards)), None, false);
        put.confirm(PeerId::random(), Status::Ok.into(), None);
        put.confirm(PeerId::random(), Status::Ok.into(), None);
        put.shard_stored.insert(0);
        put.shard_stored.insert(0);
        assert_eq!(put.result(Instant::now()), None);
        put.shard_stored.insert(1);
        assert_eq!(put.result(Instant::now()), Some(Status::Ok));
        let service = Service::new(
            crate::BootstrapStore::new(&format!(
                "/tmp/ai/test-shard-queue-{}",
                uuid::Uuid::new_v4()
            ))
            .unwrap(),
            crate::NodeConfig::default(),
        );
        let mut swarm =
            dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        let mut replicas = Replicas::default();
        replicas.puts.insert(1, put);
        let to = PeerId::random();
        for index in 0..6 {
            replicas.shard_holder(&mut swarm, &service, 1, index, to);
        }
        replicas.drain_media(&mut swarm, &service);
        assert_eq!(replicas.media_outgoing.len(), 2);
        assert_eq!(replicas.media_queue.len(), 4);
        let finished = *replicas.shard_puts.keys().next().unwrap();
        replicas.media_outgoing.remove(&finished);
        replicas.shard_puts.remove(&finished);
        replicas.drain_media(&mut swarm, &service);
        assert_eq!(replicas.media_outgoing.len(), 2);
        assert_eq!(replicas.media_queue.len(), 3);
    }

    #[tokio::test]
    async fn media_holder_status_counts_distinct_confirmations_for_the_latest_write() {
        let service = Service::new(
            crate::BootstrapStore::new(&format!(
                "/tmp/ai/test-media-holders-{}",
                uuid::Uuid::new_v4()
            ))
            .unwrap(),
            crate::NodeConfig::default(),
        );
        let mut swarm =
            dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        let mut replicas = Replicas::default();
        let a = PeerId::random();
        let b = PeerId::random();
        let mut put = PendingPut::new(
            PutRecord::Media(proto::MediaReplicaPut::default()),
            None,
            false,
        );
        put.confirm(a, Status::Ok.into(), None);
        put.confirm(a, Status::Ok.into(), None);
        put.confirm(b, Status::Full.into(), None);
        replicas.puts.insert(1, put);
        // Retain the put as background work while status takes its snapshot.
        replicas.media_puts.insert(
            swarm
                .behaviour_mut()
                .media
                .send_request(&a, proto::MediaRequest::default()),
            1,
        );
        replicas.finish_puts(&mut swarm, &service, Instant::now());
        assert_eq!(replicas.media_holders, (1, 1));
        replicas
            .puts
            .get_mut(&1)
            .unwrap()
            .confirm(b, Status::Ok.into(), None);
        replicas.finish_puts(&mut swarm, &service, Instant::now());
        assert_eq!(replicas.media_holders, (1, 2));
        let mut latest = PendingPut::new(
            PutRecord::Media(proto::MediaReplicaPut::default()),
            None,
            true,
        );
        latest.confirm(a, Status::Ok.into(), None);
        replicas.puts.insert(2, latest);
        replicas.finish_puts(&mut swarm, &service, Instant::now());
        assert_eq!(
            replicas.media_holders,
            (2, 1),
            "older writes cannot inflate the latest count"
        );
    }

    #[test]
    fn stale_profile_confirmation_must_verify_the_owner_and_version() {
        let owner = dyapp_identity::Identity::generate();
        let sign = |version| {
            dyapp_profile::Profile {
                version,
                ..Default::default()
            }
            .sign(&owner)
        };
        let mut put = PendingPut::new(PutRecord::Profile(sign(2)), None, false);
        let (a, b) = (PeerId::random(), PeerId::random());
        put.confirm(a, Status::Stale.into(), Some(sign(1)));
        let mut forged = sign(3);
        forged.signature[0] ^= 1;
        put.confirm(a, Status::Stale.into(), Some(forged));
        assert!(put.stored.is_empty());
        put.confirm(a, Status::Stale.into(), Some(sign(3)));
        put.confirm(b, Status::Stale.into(), Some(sign(2)));
        assert_eq!(put.result(Instant::now()), Some(Status::Stale));
        assert_eq!(put.stale, Some(sign(3)));
    }

    #[tokio::test]
    async fn holders_store_once_and_replica_puts_never_fan_out() {
        let dir = format!("/tmp/ai/test-fanout-{}", uuid::Uuid::new_v4());
        let mut config = crate::config::NodeConfig::default();
        config.storage.dir = dir.clone().into();
        let service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
        let mut swarm = swarm(
            Keypair::generate_ed25519(),
            &service.config.limits,
            &service.config.roles,
        )
        .unwrap();
        let me = *swarm.local_peer_id();
        let mut replicas = Replicas::default();
        let mut watchers = Watchers::new();
        let sender = dyapp_identity::Identity::generate();
        let record = sender.sign(
            dyapp_identity::Domain::Envelope,
            proto::Envelope {
                id: vec![1; 16],
                mailbox: vec![2; 32],
                ciphertext: vec![3],
            }
            .encode_to_vec(),
        );
        replicas.puts.insert(
            0,
            PendingPut::new(PutRecord::Mailbox(record.clone()), None, false),
        );
        replicas.put_holder(&mut swarm, &service, &watchers, 0, PeerId::random());
        assert_eq!(replicas.mailbox_puts.len(), 1);
        assert!(
            service.held(&[2; 32]).unwrap().is_empty(),
            "a forwarding acceptor stored a copy"
        );
        for _ in 0..REPLICAS {
            replicas.put_holder(&mut swarm, &service, &watchers, 0, me);
        }
        assert_eq!(service.held(&[2; 32]).unwrap().len(), 1);
        assert_eq!(replicas.puts[&0].stored.len(), 1);

        let connection = ConnectionId::new_unchecked(1);
        after_mailbox(
            &mut swarm,
            &service,
            &mut watchers,
            &mut replicas,
            (PeerId::random(), connection),
            Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                envelopes: vec![record.clone()],
            })),
        );
        assert!(replicas.lookups.is_empty());
        assert_eq!(
            replicas.mailbox_puts.len(),
            1,
            "replica_put forwarded again"
        );
        for id in 1..MAX_PENDING_PUTS as u64 {
            replicas.puts.insert(
                id,
                PendingPut::new(PutRecord::Mailbox(record.clone()), None, false),
            );
        }
        assert!(replicas.at_capacity());
        replicas.finish_puts(&mut swarm, &service, Instant::now() + PUT_TIMEOUT);
        assert!(!replicas.at_capacity() && replicas.puts.is_empty());
    }

    #[test]
    fn holder_selection_includes_local_and_handles_unsorted_lookup_results() {
        let me = PeerId::random();
        let nodes: Vec<_> = (0..8)
            .map(|_| kad::PeerInfo {
                peer_id: PeerId::random(),
                addrs: vec![],
            })
            .collect();
        assert_eq!(holder(me, &me.to_bytes(), &nodes), me);
        for i in 0..128u8 {
            let key = replica_key(&[i; 32], 0);
            let selected = holder(me, &key, &nodes);
            let target = kad::KBucketKey::new(key.clone());
            let distance = kad::KBucketKey::from(selected).distance(&target);
            assert!(nodes
                .iter()
                .all(|n| distance <= kad::KBucketKey::from(n.peer_id).distance(&target)));
            assert!(distance <= kad::KBucketKey::from(me).distance(&target));
        }
        assert_eq!(holder(me, &[1], &[]), me);
    }

    #[test]
    fn damaged_peer_cache_lines_are_skipped() {
        let dir = std::env::temp_dir().join(format!("dyapp-peers-{}", std::process::id()));
        assert!(cached_peers(&dir).is_empty());
        fs::write(&dir, "/ip4/1.2.3.4/tcp/1\ngarbage\n").unwrap();
        assert_eq!(cached_peers(&dir), ["/ip4/1.2.3.4/tcp/1".parse().unwrap()]);
        fs::remove_file(&dir).unwrap();
    }

    #[test]
    fn replicas_go_to_peers_seen_long_enough_that_answer() {
        let peer = PeerId::random();
        let mut trust = Trust::default();
        assert!(trust.trusted(&peer), "no delay: an unseen peer is trusted");
        trust.add(peer, -1);
        assert!(
            !trust.trusted(&peer),
            "a failed request outweighs no answer"
        );
        trust.add(peer, 1);
        trust.delay = Duration::from_secs(3600);
        assert!(!trust.trusted(&peer), "never seen");
        trust.seen.insert(peer, Instant::now());
        assert!(!trust.trusted(&peer), "seen just now");
        trust
            .seen
            .insert(peer, Instant::now() - Duration::from_secs(3600));
        assert!(trust.trusted(&peer));
        trust.retain(|_| false);
        assert!(!trust.trusted(&peer), "forgotten");
    }

    #[test]
    fn anchors_keep_the_latest_distinct_peers() {
        let peers: Vec<PeerId> = (0..5).map(|_| PeerId::random()).collect();
        let at = |i: usize| -> Multiaddr { format!("/ip4/10.0.0.{i}/tcp/1").parse().unwrap() };
        let mut anchors = Vec::new();
        for (i, peer) in peers.iter().enumerate() {
            remember_anchor(&mut anchors, *peer, &at(i));
        }
        remember_anchor(&mut anchors, peers[2], &at(9));
        let expected: Vec<Multiaddr> = [(9, 2), (4, 4), (3, 3)]
            .iter()
            .map(|&(i, p)| at(i).with(Protocol::P2p(peers[p])))
            .collect();
        assert_eq!(anchors, expected);
    }

    #[test]
    fn ip_groups_mask_the_prefix() {
        let limits = Limits::default();
        let group = |address: &str| ip_group(&address.parse().unwrap(), &limits);
        assert_eq!(group("/ip4/203.0.113.77/tcp/1"), "203.0.113.0/24");
        assert_eq!(
            group("/ip6/2001:db8:1:2::5/udp/1/quic-v1"),
            "2001:db8:1::/48"
        );
        let all = Limits {
            ipv4_prefix: 0,
            ipv6_prefix: 128,
            ..Limits::default()
        };
        assert_eq!(
            ip_group(&"/ip4/1.2.3.4".parse().unwrap(), &all),
            "0.0.0.0/0"
        );
        let ip6 = "/ip6/2001:db8::5".parse().unwrap();
        assert_eq!(ip_group(&ip6, &all), "2001:db8::5/128");
        assert_eq!(group("/memory/5"), "/memory/5");
    }
}
