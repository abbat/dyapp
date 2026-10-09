//! The libp2p request loop of a node: answers the node protocol and deletes expired envelopes.

use crate::config::{Limits, Role};
use crate::deny::DenyStore;
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
use std::time::{Duration, Instant};

/// The node's swarm with the connection and stream limits from the config. `/dyapp/kad` is the
/// key space of the store role: a node without it is a DHT client and serves no protocol, so it
/// is never picked as a replica.
/// ponytail: no memory-use threshold (libp2p memory-connection-limits is not vendored); the
/// connection, stream and message caps bound memory instead.
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
    build_limited_swarm(keypair, mode, connections, limits.max_streams)
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

/// A lookup of a mailbox's replica key and what follows it.
enum Lookup {
    /// A device's ack, forwarded to the closest node.
    Ack(SignedRecord),
    /// A mailbox whose inventory goes to the closest node.
    Repair(Vec<u8>),
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

/// Node-to-node mailbox traffic: ack forwarding and repair.
#[derive(Default)]
struct Replicas {
    trust: Trust,
    lookups: HashMap<kad::QueryId, Lookup>,
    /// Inventories sent and their mailbox.
    inventories: HashMap<OutboundRequestId, Vec<u8>>,
    /// Mailboxes repaired since the last hourly cleanup.
    /// ponytail: cleared hourly, so a mailbox may be repaired twice within an hour.
    repaired: HashSet<Vec<u8>>,
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
            if service.traffic.add(0).max(service.traffic.second()) < SHED_MEDIA
                && repaired.insert(mailbox.clone())
            {
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
    // Requests answered and failed with a node error since the last maintenance run.
    let (mut answered, mut failed) = (0u64, 0u64);
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
        .filter_map(|s| s.parse().ok())
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
                // ponytail: SQLite calls block the swarm loop; move them to spawn_blocking when
                // load makes request latency visible.
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                    message: Message::Request { request, channel, .. },
                    ..
                })) => {
                    let response = service.node(request);
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
                    match service.profile(&peer(id, &groups, connection_id), request) {
                        Ok(response) => {
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
                    match service.media(&peer(id, &groups, connection_id), request) {
                        Ok(response) => {
                            answered += 1;
                    service.traffic.add(response.encoded_len() as u64);
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
                    result: kad::QueryResult::GetClosestPeers(result),
                    ..
                })) => {
                    let lookup = replicas.lookups.remove(&id);
                    if let (Some(lookup), Ok(ok)) = (lookup, result) {
                        // A peer without the node-ID proof of work never holds a replica.
                        let bits = dyapp_p2p_net::id_pow_bits();
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
                }
                SwarmEvent::IncomingConnectionError { error: ListenError::Denied { .. }, .. } => {
                    refused += 1;
                }
                _ => {}
            }
            },
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
                if let Err(error) = service.traffic.save() {
                    tracing::error!(%error, "traffic count not saved");
                }
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
                    tracing::warn!(refused, "connections refused by the connection limits");
                    refused = 0;
                }
                status(&service, &swarm, peers.len(), answered, failed);
                (answered, failed) = (0, 0);
                dyapp_p2p_net::join(&mut swarm, &seeds, &[]);
            }
        }
    }
}

/// The hourly status line: no peer or key identifiers, counts and sizes only.
fn status(service: &Service, swarm: &Swarm<Behaviour>, known: usize, answered: u64, failed: u64) {
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
        traffic_percent = service.traffic.add(0),
        "node status"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
