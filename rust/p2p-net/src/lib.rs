//! libp2p node: QUIC and TCP+Noise+Yamux over DNS, Kademlia, identify, AutoNAT and the node
//! protocol (`/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push`,
//! `/dyapp/media`) over request-response with protobuf messages. A search node adds
//! `/dyapp/search-kad`, gossipsub, `/dyapp/search` and `/dyapp/inventory`.

use libp2p::futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{
    autonat, connection_limits, gossipsub, identify, kad, memory_connection_limits, noise, tcp,
    yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use std::io;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

mod behaviour;
pub use behaviour::{Behaviour, BehaviourEvent};
mod dial_filter;
pub use dial_filter::{DenyDial, DialFilter};
mod dial_guard;
pub use dial_guard::{DialGuard, DialSlot};
#[cfg(test)]
mod endpoint_tag;
mod guard_dial;
pub use guard_dial::GuardDial;
pub use ipnet::IpNet;
mod incoming_groups;
pub use incoming_groups::{IncomingGroupGuard, IncomingGroupLimits};
mod pow_gate;
pub use pow_gate::PowGate;
mod protocols;
pub use protocols::Protocols;
mod wire;
pub use wire::{Dir, WireBytes, WireCounts};

/// Node protocol messages, generated from proto/node.proto.
pub mod proto {
    #![allow(clippy::pedantic)]
    // prost emits no docs for messages without proto comments.
    #![allow(missing_docs)]
    include!(concat!(env!("OUT_DIR"), "/dyapp.node.rs"));
}

/// Kademlia key space of the store role.
pub const KAD_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/kad");
/// Identify protocol version and ID prefix of the public network.
pub const IDENTIFY_PROTOCOL: &str = "/dyapp";
/// Node control requests.
pub const NODE_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/node");
/// Profile publication and lookup.
pub const PROFILE_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/profile");
/// Mailbox store, fetch and ack.
pub const MAILBOX_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/mailbox");
/// Node-to-client mailbox notifications.
pub const MAILBOX_PUSH_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/mailbox-push");
/// Media blob transfer.
pub const MEDIA_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/media");
/// Key space of the search role, separate from the store role's `/dyapp/kad`.
pub const SEARCH_KAD_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/search-kad");
/// Search queries to search nodes.
pub const SEARCH_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/search");
/// Search index inventory exchange between search nodes.
pub const INVENTORY_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/inventory");
/// Reserved for search bucket digests; no behaviour serves it yet.
pub const DIGEST_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/digest");
/// Provider key under which search nodes announce themselves in the main `/dyapp/kad`.
pub const SEARCH_KEY: &[u8] = b"dyapp/search";
/// Gossip frame limit: a profile record of at most 1 MiB plus framing.
pub const MAX_GOSSIP_BYTES: usize = (1 << 20) + 4096;

/// Gossipsub topic `n` of the search index (`/dyapp/profiles/<n>`).
pub fn search_topic(n: u32) -> gossipsub::IdentTopic {
    gossipsub::IdentTopic::new(format!("/dyapp/profiles/{n}"))
}

/// Kademlia provider key under which nodes with the turn role announce their relay.
pub const TURN_KEY: &[u8] = b"/dyapp/turn";

/// Replicas of every profile, envelope and signal (ADR 0009).
pub const REPLICAS: u8 = 5;

/// Kademlia lookup key of replica `i` (`0..REPLICAS`) of `key`: Kademlia hashes it with SHA-256,
/// so the replica lives on the store nodes closest to H(key ‖ i). The acceptor looks up every holder and writes the replicas.
pub fn replica_key(key: &[u8], i: u8) -> Vec<u8> {
    [key, &[i]].concat()
}

/// Non-media frame limit, including protobuf and signature overhead.
pub const MAX_CONTROL_MESSAGE_BYTES: u64 = 1024 * 1024 + 64 * 1024;
/// Media frame limit, including protobuf and signature overhead.
pub const MAX_MEDIA_MESSAGE_BYTES: u64 = 6 * 1024 * 1024 + 64 * 1024;

fn message_limit(protocol: &StreamProtocol, protocols: &Protocols) -> u64 {
    if protocol == &protocols.media {
        MAX_MEDIA_MESSAGE_BYTES
    } else {
        MAX_CONTROL_MESSAGE_BYTES
    }
}

/// Request-response behaviour of [`NODE_PROTOCOL`].
pub type NodeBehaviour =
    request_response::Behaviour<ProtoCodec<proto::NodeRequest, proto::NodeResponse>>;
/// Request-response behaviour of [`PROFILE_PROTOCOL`].
pub type ProfileBehaviour =
    request_response::Behaviour<ProtoCodec<proto::ProfileRequest, proto::ProfileResponse>>;
/// Request-response behaviour of [`MAILBOX_PROTOCOL`].
pub type MailboxBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MailboxRequest, proto::MailboxResponse>>;
/// Request-response behaviour of [`MAILBOX_PUSH_PROTOCOL`].
pub type PushBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MailboxPush, proto::MailboxPushResponse>>;
/// Request-response behaviour of [`MEDIA_PROTOCOL`].
pub type MediaBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MediaRequest, proto::MediaResponse>>;
/// Request-response behaviour of [`SEARCH_PROTOCOL`].
pub type SearchBehaviour =
    request_response::Behaviour<ProtoCodec<proto::SearchRequest, proto::SearchResponse>>;
/// Request-response behaviour of [`INVENTORY_PROTOCOL`].
pub type InventoryBehaviour =
    request_response::Behaviour<ProtoCodec<proto::InventoryRequest, proto::InventoryResponse>>;

/// One protobuf message per stream; the writer closes the stream after it.
pub struct ProtoCodec<Req, Resp> {
    marker: PhantomData<fn() -> (Req, Resp)>,
    wire: Arc<WireBytes>,
    protocols: Arc<Protocols>,
    /// Shared by every connection of a media behaviour; other protocols have no permit pool.
    permits: Option<Arc<Semaphore>>,
    /// Held from before reading a request until its response is sent or the stream is dropped.
    permit: Option<OwnedSemaphorePermit>,
}

impl<Req, Resp> ProtoCodec<Req, Resp> {
    /// Shares successful protobuf frame counts across codecs and connections.
    pub fn with_wire(mut self, wire: Arc<WireBytes>) -> Self {
        self.wire = wire;
        self
    }

    /// Configures exact protocol IDs for frame limits and stable wire counter labels.
    pub fn with_protocols(mut self, protocols: Arc<Protocols>) -> Self {
        self.protocols = protocols;
        self
    }

    fn media() -> Self {
        Self {
            permits: Some(Arc::new(Semaphore::new(16))),
            ..Self::default()
        }
    }
}

impl<Req, Resp> Default for ProtoCodec<Req, Resp> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
            wire: Arc::default(),
            protocols: Arc::new(Protocols::public()),
            permits: None,
            permit: None,
        }
    }
}

impl<Req, Resp> Clone for ProtoCodec<Req, Resp> {
    fn clone(&self) -> Self {
        Self {
            marker: PhantomData,
            permits: self.permits.clone(),
            wire: self.wire.clone(),
            protocols: self.protocols.clone(),
            permit: None,
        }
    }
}

async fn read_message<M: prost::Message + Default, T: AsyncRead + Unpin + Send>(
    io: &mut T,
    limit: u64,
    response: bool,
) -> io::Result<(M, usize)> {
    let mut buf = Vec::new();
    // One byte over the limit tells an oversized message from one that is exactly at it.
    io.take(limit + 1).read_to_end(&mut buf).await?;
    if buf.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    if response && buf.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "stream closed without a response",
        ));
    }
    M::decode(buf.as_slice())
        .map(|message| (message, buf.len()))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

async fn write_message<M: prost::Message, T: AsyncWrite + Unpin + Send>(
    io: &mut T,
    message: M,
) -> io::Result<usize> {
    let bytes = message.encode_to_vec();
    io.write_all(&bytes).await?;
    Ok(bytes.len())
}

impl<Req, Resp> request_response::Codec for ProtoCodec<Req, Resp>
where
    Req: prost::Message + Default + Send,
    Resp: prost::Message + Default + Send,
{
    type Protocol = StreamProtocol;
    type Request = Req;
    type Response = Resp;

    async fn read_request<T: AsyncRead + Unpin + Send>(
        &mut self,
        protocol: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Req> {
        if let Some(permits) = &self.permits {
            self.permit = Some(permits.clone().try_acquire_owned().map_err(|_| {
                io::Error::new(io::ErrorKind::WouldBlock, "media streams exhausted")
            })?);
        }
        let result = read_message(io, message_limit(protocol, &self.protocols), false)
            .await
            .map(|(message, n)| {
                self.wire.add(
                    WireBytes::protocol_slot_for(protocol, &self.protocols),
                    Dir::In,
                    n,
                );
                message
            });
        if result.is_err() {
            self.permit.take();
        }
        result
    }

    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        protocol: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Resp> {
        let (message, n) = read_message(io, message_limit(protocol, &self.protocols), true).await?;
        self.wire.add(
            WireBytes::protocol_slot_for(protocol, &self.protocols),
            Dir::In,
            n,
        );
        Ok(message)
    }

    async fn write_request<T: AsyncWrite + Unpin + Send>(
        &mut self,
        protocol: &StreamProtocol,
        io: &mut T,
        request: Req,
    ) -> io::Result<()> {
        let n = write_message(io, request).await?;
        self.wire.add(
            WireBytes::protocol_slot_for(protocol, &self.protocols),
            Dir::Out,
            n,
        );
        Ok(())
    }

    async fn write_response<T: AsyncWrite + Unpin + Send>(
        &mut self,
        protocol: &StreamProtocol,
        io: &mut T,
        response: Resp,
    ) -> io::Result<()> {
        let result = write_message(io, response).await.map(|n| {
            self.wire.add(
                WireBytes::protocol_slot_for(protocol, &self.protocols),
                Dir::Out,
                n,
            );
        });
        self.permit.take();
        result
    }
}

/// Kademlia role of the node.
pub enum Mode {
    /// DHT server once AutoNAT or the operator confirms an external address.
    Auto,
    /// Never answers DHT queries: mobile and other short-lived clients.
    Client,
}

/// Settings shared by client and node swarm constructors.
pub struct SwarmOptions {
    /// Kademlia role.
    pub mode: Mode,
    /// Global and per-peer connection limits.
    pub connection_limits: connection_limits::ConnectionLimits,
    /// Concurrent streams per connection and protocol.
    pub max_streams: usize,
    /// Physical memory above which new connections are refused.
    pub max_memory_bytes: usize,
    /// Serve search protocols and gossip.
    pub search: bool,
    /// Optional incoming-only connection budget per remote IP group.
    pub incoming_groups: Option<IncomingGroupLimits>,
    /// Successful protobuf payload bytes, shared with the node service.
    pub wire: Arc<WireBytes>,
    /// Network negotiation IDs; public unless explicitly supplied.
    pub protocols: Arc<Protocols>,
}

impl Default for SwarmOptions {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            connection_limits: connection_limits::ConnectionLimits::default(),
            max_streams: 100,
            max_memory_bytes: usize::MAX,
            search: false,
            incoming_groups: None,
            wire: Arc::default(),
            protocols: Arc::new(Protocols::public()),
        }
    }
}

/// A swarm with default options in `mode`.
pub fn build_swarm(keypair: Keypair, mode: Mode) -> anyhow::Result<Swarm<Behaviour>> {
    build_configured_swarm(
        keypair,
        SwarmOptions {
            mode,
            ..SwarmOptions::default()
        },
    )
}

/// [`build_swarm`] with connection limits and at most `max_streams` concurrent streams per
/// connection and protocol; new connections are refused while the process uses more than
/// `max_memory_bytes` of physical memory. `search` serves the search protocols and gossip.
pub fn build_limited_swarm(
    keypair: Keypair,
    mode: Mode,
    limits: connection_limits::ConnectionLimits,
    max_streams: usize,
    max_memory_bytes: usize,
    search: bool,
) -> anyhow::Result<Swarm<Behaviour>> {
    build_configured_swarm(
        keypair,
        SwarmOptions {
            mode,
            connection_limits: limits,
            max_streams,
            max_memory_bytes,
            search,
            incoming_groups: None,
            wire: Arc::default(),
            protocols: Arc::new(Protocols::public()),
        },
    )
}

/// Builds a swarm with explicit settings; legacy constructors delegate here.
pub fn build_configured_swarm(
    keypair: Keypair,
    options: SwarmOptions,
) -> anyhow::Result<Swarm<Behaviour>> {
    let SwarmOptions {
        mode,
        connection_limits: limits,
        max_streams,
        max_memory_bytes,
        search,
        incoming_groups,
        wire,
        protocols,
    } = options;
    Ok(SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_dns()?
        .with_behaviour(|key| {
            let peer_id = key.public().to_peer_id();
            let mut kad_config = kad::Config::new(protocols.kad.clone());
            // Every insert goes through `add_peer`, so the diversity limits hold (ADR 0008).
            kad_config
                .disjoint_query_paths(true)
                .set_kbucket_inserts(kad::BucketInserts::Manual);
            let mut kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                kad_config,
            );
            let (kad_mode, support, push) = match mode {
                Mode::Auto => (None, ProtocolSupport::Full, ProtocolSupport::Outbound),
                Mode::Client => (
                    Some(kad::Mode::Client),
                    ProtocolSupport::Outbound,
                    ProtocolSupport::Inbound,
                ),
            };
            kad.set_mode(kad_mode);
            let mut search_kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                kad::Config::new(protocols.search_kad.clone()),
            );
            search_kad.set_mode(Some(if search {
                kad::Mode::Server
            } else {
                kad::Mode::Client
            }));
            let gossip = search.then(|| {
                let mut config = gossipsub::ConfigBuilder::default();
                if let Some(prefix) = &protocols.gossip_prefix {
                    config.protocol_id_prefix(prefix.clone());
                }
                let config = config
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .validate_messages()
                    .max_transmit_size(MAX_GOSSIP_BYTES)
                    .message_id_fn(|m: &gossipsub::Message| {
                        gossipsub::MessageId::from(dyapp_identity::sha256(&m.data).to_vec())
                    })
                    .build()
                    .expect("valid gossipsub config");
                gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    config,
                )
                .expect("valid gossipsub behaviour")
            });
            let search_support = if search {
                ProtocolSupport::Full
            } else {
                ProtocolSupport::Outbound
            };
            let config = request_response::Config::default()
                .with_request_timeout(Duration::from_secs(20))
                .with_max_concurrent_streams(max_streams);
            Behaviour {
                limits: connection_limits::Behaviour::new(limits),
                memory: memory_connection_limits::Behaviour::with_max_bytes(max_memory_bytes),
                incoming_groups: incoming_groups.map(IncomingGroupGuard::new).into(),
                kad,
                identify: identify::Behaviour::new(identify::Config::new(
                    protocols.version.clone(),
                    key.public(),
                )),
                autonat: autonat::Behaviour::new(peer_id, autonat::Config::default()),
                node: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.node.clone(), support.clone())],
                    config.clone(),
                ),
                profile: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.profile.clone(), support.clone())],
                    config.clone(),
                ),
                mailbox: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.mailbox.clone(), support.clone())],
                    config.clone(),
                ),
                push: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.mailbox_push.clone(), push)],
                    config.clone(),
                ),
                search_kad,
                gossip: gossip.into(),
                search: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.search.clone(), search_support.clone())],
                    config.clone(),
                ),
                inventory: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.inventory.clone(), search_support)],
                    config.clone(),
                ),
                media: request_response::Behaviour::with_codec(
                    ProtoCodec::media()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.media.clone(), support)],
                    config.with_max_concurrent_streams(max_streams.min(2)),
                ),
            }
        })?
        // Happy Eyeballs (RFC 8305): QUIC before TCP, IPv6 before IPv4, which starts 250 ms
        // later on public addresses and 30 ms later on private ones.
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(Duration::from_secs(60))
                .with_smart_dial()
        })
        .build())
}

/// Leading zero bits of SHA-256 over a node's peer ID bytes: the static S/Kademlia puzzle that
/// makes every node ID cost about a minute on 2 vCPU (ADR 0008).
pub const ID_POW_BITS: u32 = 22;

// The unit tests route random peer IDs.
static POW_BITS: AtomicU32 = AtomicU32::new(if cfg!(test) { 0 } else { ID_POW_BITS });

/// Overrides [`ID_POW_BITS`] for this process: a test network sets it low.
pub fn set_id_pow_bits(bits: u32) {
    POW_BITS.store(bits, Ordering::Relaxed);
}

/// Required leading zero bits of a peer ID proof of work in this process.
pub fn id_pow_bits() -> u32 {
    POW_BITS.load(Ordering::Relaxed)
}

/// Whether `peer` carries the node-ID proof of work of `bits` leading zero bits.
pub fn id_has_pow(peer: &PeerId, bits: u32) -> bool {
    let hash = dyapp_identity::sha256(&peer.to_bytes());
    let zeros = hash
        .iter()
        .position(|b| *b != 0)
        .map_or(256, |i| i as u32 * 8 + hash[i].leading_zeros());
    zeros >= bits
}

/// A fresh Ed25519 key whose peer ID has the proof of work of `bits`, searched on every CPU.
pub fn generate_pow_keypair(bits: u32) -> Keypair {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let found = AtomicBool::new(false);
    std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    while !found.load(Ordering::Relaxed) {
                        let keypair = Keypair::generate_ed25519();
                        if id_has_pow(&keypair.public().to_peer_id(), bits) {
                            found.store(true, Ordering::Relaxed);
                            return Some(keypair);
                        }
                    }
                    None
                })
            })
            .collect();
        workers
            .into_iter()
            .find_map(|w| w.join().expect("key search thread panicked"))
            .expect("a worker found the key")
    })
}

/// Starts joining the network: adds `cached` peers (`.../p2p/<id>` addresses from
/// [`known_peers`]) to the routing table and dials every seed. Kademlia bootstraps on its own once the first peer is in the table.
pub fn join(swarm: &mut Swarm<Behaviour>, seeds: &[Multiaddr], cached: &[Multiaddr]) {
    for address in cached {
        if let Some(Protocol::P2p(peer)) = address.iter().last() {
            add_peer(swarm, peer, address.clone());
        }
    }
    for seed in seeds {
        if let Err(error) = swarm.dial(seed.clone()) {
            tracing::warn!(%seed, %error, "seed not dialed");
        }
    }
}

/// Peers of one IP group (/24, IPv6 /48, or one DNS name) allowed per k-bucket and per routing
/// table (ADR 0008).
pub const MAX_GROUP_PER_BUCKET: usize = 2;
/// Peers of one IP group allowed per routing table.
pub const MAX_GROUP_PER_TABLE: usize = 10;

static DISTINCT_GROUPS: AtomicBool = AtomicBool::new(true);

/// Whether a k-bucket takes at most one peer per wide group (/16, IPv6 /32, or one DNS name), so
/// the peers Kademlia dials for one key range sit in distinct networks (ADR 0008). On by
/// default; a node on a trusted network, where every peer shares a /16, turns it off.
pub fn set_distinct_groups(on: bool) {
    DISTINCT_GROUPS.store(on, Ordering::Relaxed);
}

/// Whether two direct addresses name exactly the same IP, independent of port and transport.
/// DNS, relayed routes and multiple IP components cannot prove a direct remote endpoint.
pub fn same_ip(observed: &Multiaddr, claimed: &Multiaddr) -> bool {
    fn direct_ip(address: &Multiaddr) -> Option<std::net::IpAddr> {
        let mut parts = address.iter();
        let ip = match parts.next()? {
            Protocol::Ip4(ip) => ip.into(),
            Protocol::Ip6(ip) => ip.into(),
            _ => return None,
        };
        if parts.any(|p| {
            matches!(
                p,
                Protocol::Ip4(_)
                    | Protocol::Ip6(_)
                    | Protocol::Dns(_)
                    | Protocol::Dns4(_)
                    | Protocol::Dns6(_)
                    | Protocol::Dnsaddr(_)
                    | Protocol::P2pCircuit
            )
        }) {
            return None;
        }
        Some(ip)
    }
    direct_ip(observed).is_some_and(|ip| direct_ip(claimed) == Some(ip))
}

/// The IP group of `address`: /24 (IPv6 /48), or /16 (IPv6 /32) when `wide`; a DNS name is its
/// own group.
fn ip_group(address: &Multiaddr, wide: bool) -> Option<String> {
    address.iter().find_map(|p| match p {
        Protocol::Ip4(ip) => {
            let [a, b, c, _] = ip.octets();
            Some(if wide {
                format!("{a}.{b}")
            } else {
                format!("{a}.{b}.{c}")
            })
        }
        Protocol::Ip6(ip) => {
            let s = ip.segments();
            Some(if wide {
                format!("{:x}:{:x}", s[0], s[1])
            } else {
                format!("{:x}:{:x}:{:x}", s[0], s[1], s[2])
            })
        }
        Protocol::Dns(h) | Protocol::Dns4(h) | Protocol::Dns6(h) | Protocol::Dnsaddr(h) => {
            Some(h.to_string())
        }
        _ => None,
    })
}

/// Adds `peer` at `address` to the routing table unless its IP group already has
/// [`MAX_GROUP_PER_BUCKET`] other peers in the peer's bucket or [`MAX_GROUP_PER_TABLE`] in the
/// table, or, with [`set_distinct_groups`] on, its /16 already has a peer in the bucket. A peer
/// without the node-ID proof of work ([`id_pow_bits`]) is never added: it is
/// served, but never routed to or given replicas. A full bucket keeps its oldest live peers: the
/// newcomer only replaces one that stopped answering. Returns whether the address was passed to
/// Kademlia.
pub fn add_peer(swarm: &mut Swarm<Behaviour>, peer: PeerId, address: Multiaddr) -> bool {
    if !id_has_pow(&peer, id_pow_bits()) {
        tracing::debug!(%peer, "peer ID without proof of work not routed");
        return false;
    }
    let kad = &mut swarm.behaviour_mut().kad;
    if let (Some(group), Some(wide)) = (ip_group(&address, false), ip_group(&address, true)) {
        let Some(range) = kad.kbucket(peer).map(|b| b.range()) else {
            return false;
        };
        let distinct = DISTINCT_GROUPS.load(Ordering::Relaxed);
        let (mut bucket_count, mut table_count, mut bucket_wide) = (0, 0, false);
        for bucket in kad.kbuckets() {
            let in_bucket = bucket.range() == range;
            for entry in bucket.iter() {
                if *entry.node.key.preimage() == peer {
                    continue;
                }
                let has = |g: &str, w| {
                    entry
                        .node
                        .value
                        .iter()
                        .any(|a| ip_group(a, w).as_deref() == Some(g))
                };
                if has(&group, false) {
                    table_count += 1;
                    bucket_count += usize::from(in_bucket);
                }
                bucket_wide |= in_bucket && has(&wide, true);
            }
        }
        if bucket_count >= MAX_GROUP_PER_BUCKET
            || table_count >= MAX_GROUP_PER_TABLE
            || (distinct && bucket_wide)
        {
            tracing::debug!(%peer, %address, "routing table full for this IP group");
            return false;
        }
    }
    kad.add_address(&peer, address);
    true
}

/// Feeds a peer Kademlia found routable (a dialled peer, a lookup result) through [`add_peer`];
/// call it on every swarm event.
pub fn route(swarm: &mut Swarm<Behaviour>, event: &SwarmEvent<BehaviourEvent>) {
    if let SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::RoutablePeer { peer, address })) =
        event
    {
        add_peer(swarm, *peer, address.clone());
    }
}

/// Whether `peer` is in the routing table.
pub fn routed(swarm: &mut Swarm<Behaviour>, peer: &PeerId) -> bool {
    let kad = &mut swarm.behaviour_mut().kad;
    kad.kbucket(*peer)
        .is_some_and(|b| b.iter().any(|e| e.node.key.preimage() == peer))
}

/// Every address in the routing table as `.../p2p/<id>`, for a peer cache that lets the next
/// start join without any one seed.
pub fn known_peers(swarm: &mut Swarm<Behaviour>) -> Vec<Multiaddr> {
    let mut peers = Vec::new();
    for bucket in swarm.behaviour_mut().kad.kbuckets() {
        for entry in bucket.iter() {
            let peer = *entry.node.key.preimage();
            for address in entry.node.value.iter() {
                let mut address = address.clone();
                if !matches!(address.iter().last(), Some(Protocol::P2p(_))) {
                    address.push(Protocol::P2p(peer));
                }
                peers.push(address);
            }
        }
    }
    peers
}

#[cfg(test)]
#[path = "lib/media_streams.rs"]
mod media_stream_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_ip_compares_direct_hosts_not_groups_or_ports() {
        let addr = |s: &str| s.parse::<Multiaddr>().unwrap();
        for (a, b, expected) in [
            ("/ip4/1.2.3.4/tcp/1", "/ip4/1.2.3.4/udp/9/quic-v1", true),
            ("/ip4/1.2.3.4/tcp/1", "/ip4/1.2.3.5/tcp/1", false),
            ("/ip6/2001:db8::1/tcp/1", "/ip6/2001:db8::1/tcp/2", true),
            ("/ip6/2001:db8::1/tcp/1", "/ip6/2001:db8::2/tcp/1", false),
            ("/ip4/1.2.3.4/tcp/1", "/ip6/::ffff:1.2.3.4/tcp/1", false),
        ] {
            assert_eq!(same_ip(&addr(a), &addr(b)), expected, "{a} vs {b}");
            assert_eq!(same_ip(&addr(b), &addr(a)), expected, "{b} vs {a}");
        }
        let direct = addr("/ip4/1.2.3.4/tcp/1");
        let pinned = direct.clone().with(Protocol::P2p(PeerId::random()));
        assert!(same_ip(&direct, &pinned));
        for unproven in [
            "",
            "/tcp/1",
            "/dns/example.org/tcp/1",
            "/dns4/example.org/tcp/1",
            "/dns6/example.org/tcp/1",
            "/dnsaddr/example.org",
            "/tcp/1/ip4/1.2.3.4",
            "/ip4/1.2.3.4/dns/example.org/tcp/1",
            "/ip4/1.2.3.4/ip4/1.2.3.4/tcp/1",
            "/ip4/1.2.3.4/tcp/1/p2p-circuit",
        ] {
            let unproven = addr(unproven);
            assert!(!same_ip(&direct, &unproven), "{unproven}");
            assert!(!same_ip(&unproven, &direct), "{unproven}");
            assert!(!same_ip(&unproven, &unproven), "{unproven}");
        }
    }

    #[derive(Clone, PartialEq, prost::Message)]
    struct Frame {
        #[prost(bytes = "vec", tag = "1")]
        data: Vec<u8>,
    }

    #[test]
    fn old_heartbeat_decodes_with_version_zero() {
        use prost::Message;
        // Field 2 is absent on the wire when it is zero, as from a client older than it.
        let old = proto::Heartbeat {
            time: 7,
            version: 0,
        }
        .encode_to_vec();
        assert_eq!(proto::Heartbeat::decode(old.as_slice()).unwrap().version, 0);
    }

    fn frame_bytes(size: usize) -> Vec<u8> {
        use prost::Message;
        let mut frame = Frame {
            data: vec![0; size],
        };
        let overhead = frame.encoded_len() - size;
        frame.data.truncate(size - overhead);
        let bytes = frame.encode_to_vec();
        assert_eq!(bytes.len(), size);
        bytes
    }

    /// A reader that fails the test if admission touches any request-body bytes.
    struct UnreadBody;

    impl AsyncRead for UnreadBody {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut [u8],
        ) -> std::task::Poll<io::Result<usize>> {
            panic!("rejected media stream read its body");
        }
    }

    #[tokio::test]
    async fn media_permits_are_shared_and_reject_before_reading() {
        use libp2p::futures::io::Cursor;
        use request_response::Codec;
        let codec = ProtoCodec::<Frame, Frame>::media();
        let mut streams = Vec::new();
        for _ in 0..16 {
            let mut stream = codec.clone();
            stream
                .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
                .await
                .unwrap();
            streams.push(stream);
        }
        let mut extra = codec.clone();
        assert_eq!(
            extra
                .read_request(&MEDIA_PROTOCOL, &mut UnreadBody)
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        // A completed response frees a slot, independently of the connection's codec clone.
        streams[0]
            .write_response(
                &MEDIA_PROTOCOL,
                &mut Cursor::new(Vec::new()),
                Frame::default(),
            )
            .await
            .unwrap();
        extra
            .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
            .await
            .unwrap();
        assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 0);
        drop(extra);
        // Malformed requests must return their permit as well.
        let mut malformed = codec.clone();
        assert!(malformed
            .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(vec![0xff]))
            .await
            .is_err());
        assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 1);
        drop(streams);
        assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 16);
        // A different swarm has its own pool.
        let independent = ProtoCodec::<Frame, Frame>::media();
        assert_eq!(
            independent.permits.as_ref().unwrap().available_permits(),
            16
        );
    }

    #[tokio::test]
    async fn named_builder_announces_only_its_network_and_exchanges_large_media_over_tcp() {
        use prost::Message;
        let protocols = Arc::new(Protocols::new("builder-test").unwrap());
        let a_wire = Arc::new(WireBytes::default());
        let b_wire = Arc::new(WireBytes::default());
        let make = |mode, wire| {
            build_configured_swarm(
                Keypair::generate_ed25519(),
                SwarmOptions {
                    mode,
                    search: true,
                    protocols: protocols.clone(),
                    wire,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let mut a = make(Mode::Auto, a_wire.clone());
        let mut b = make(Mode::Client, b_wire.clone());
        a.behaviour_mut().kad.set_mode(Some(kad::Mode::Server));
        a.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = a.select_next_some().await {
                break address;
            }
        };
        let request = proto::MediaRequest {
            request: Some(proto::media_request::Request::Put(proto::MediaPut {
                owner: vec![1; 32],
                data: vec![7; 2 << 20],
            })),
        };
        let request_len = request.encoded_len() as u64;
        let expected_response = proto::MediaResponse {
            status: proto::Status::Ok.into(),
            data: vec![7; 2 << 20],
            ..Default::default()
        };
        let response_len = expected_response.encoded_len() as u64;
        let check_identify = |info: &identify::Info, server: bool| {
            assert_eq!(info.protocol_version, protocols.version);
            let ids = info
                .protocols
                .iter()
                .map(|id| id.as_ref())
                .collect::<Vec<_>>();
            for id in &ids {
                if id.starts_with("/dyapp") || id.starts_with("/meshsub") {
                    assert!(
                        id.starts_with(&format!("{}/", protocols.version)),
                        "public protocol leaked: {id}"
                    );
                }
            }
            if server {
                for id in protocols
                    .all()
                    .into_iter()
                    .filter(|id| *id != &protocols.mailbox_push)
                {
                    assert!(ids.contains(&id.as_ref()), "not advertised: {id}");
                }
            } else {
                assert!(ids.contains(&protocols.mailbox_push.as_ref()));
            }
            assert!(ids
                .iter()
                .any(|id| id.starts_with(protocols.gossip_prefix.as_ref().unwrap())));
        };
        b.dial(address).unwrap();
        let (mut a_identified, mut b_identified, mut received) = (false, false, false);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !(a_identified && b_identified && received) {
                tokio::select! {
                    event = a.select_next_some() => match event {
                        SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { info, .. })) => {
                            check_identify(&info, false);
                            a_identified = true;
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::Media(request_response::Event::Message {
                            message: request_response::Message::Request { request, channel, .. }, ..
                        })) => {
                            let Some(proto::media_request::Request::Put(put)) = request.request else { panic!("not media put") };
                            a.behaviour_mut().media.send_response(channel, proto::MediaResponse {
                                status: proto::Status::Ok.into(), data: put.data, ..Default::default()
                            }).unwrap();
                        }
                        _ => {}
                    },
                    event = b.select_next_some() => match event {
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            b.behaviour_mut().media.send_request(&peer_id, request.clone());
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { info, .. })) => {
                            check_identify(&info, true);
                            b_identified = true;
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::Media(request_response::Event::Message {
                            message: request_response::Message::Response { response, .. }, ..
                        })) => {
                            assert_eq!(response, expected_response);
                            received = true;
                        }
                        _ => {}
                    }
                }
            }
        }).await.unwrap();
        let mut expected = [[0; 2]; 10];
        expected[4] = [request_len, response_len];
        assert_eq!(a_wire.take().0, expected);
        expected[4] = [response_len, request_len];
        assert_eq!(b_wire.take().0, expected);
    }

    #[tokio::test]
    async fn search_nodes_gossip() {
        let search_swarm = || {
            let limits = connection_limits::ConnectionLimits::default();
            let keypair = Keypair::generate_ed25519();
            let mut s = build_limited_swarm(keypair, Mode::Auto, limits, 64, usize::MAX, true);
            let gossip = s.as_mut().unwrap().behaviour_mut().gossip.as_mut().unwrap();
            gossip.subscribe(&search_topic(3)).unwrap();
            s.unwrap()
        };
        let (mut a, mut b) = (search_swarm(), search_swarm());
        a.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let addr = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = a.select_next_some().await {
                break address;
            }
        };
        b.dial(addr).unwrap();
        let got = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                tokio::select! {
                    e = a.select_next_some() => {
                        if let SwarmEvent::Behaviour(BehaviourEvent::Gossip(
                            gossipsub::Event::Subscribed { .. },
                        )) = e
                        {
                            let gossip = a.behaviour_mut().gossip.as_mut().unwrap();
                            gossip.publish(search_topic(3), b"hello".to_vec()).unwrap();
                        }
                    }
                    e = b.select_next_some() => {
                        if let SwarmEvent::Behaviour(BehaviourEvent::Gossip(
                            gossipsub::Event::Message { message, .. },
                        )) = e
                        {
                            break message.data;
                        }
                    }
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(got, b"hello");
        let plain = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        assert!(plain.behaviour().gossip.as_ref().is_none());
    }

    #[tokio::test]
    async fn configured_media_ids_keep_large_frames_and_stable_counter_slots() {
        use libp2p::futures::io::Cursor;
        use prost::Message;
        use request_response::Codec;
        for protocols in [Protocols::public(), Protocols::new("codec-test").unwrap()] {
            let protocols = Arc::new(protocols);
            let wire = Arc::new(WireBytes::default());
            let mut codec = ProtoCodec::<Frame, Frame>::media()
                .with_wire(wire.clone())
                .with_protocols(protocols.clone());
            let frame = Frame {
                data: vec![0; 2 << 20],
            };
            let bytes = frame.encode_to_vec();
            codec
                .read_request(&protocols.media, &mut Cursor::new(&bytes))
                .await
                .unwrap();
            let mut cloned = codec.clone();
            assert!(Arc::ptr_eq(&cloned.protocols, &protocols));
            assert!(cloned.permit.is_none());
            cloned
                .read_response(&protocols.media, &mut Cursor::new(&bytes))
                .await
                .unwrap();
            cloned
                .write_request(
                    &protocols.media,
                    &mut Cursor::new(Vec::new()),
                    frame.clone(),
                )
                .await
                .unwrap();
            codec
                .write_response(&protocols.media, &mut Cursor::new(Vec::new()), frame)
                .await
                .unwrap();
            let mut expected = [[0; 2]; 10];
            expected[4] = [2 * bytes.len() as u64; 2];
            assert_eq!(wire.take().0, expected);
            assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 16);
            for foreign in [&protocols.node, &StreamProtocol::new("/unknown/media")] {
                assert!(codec
                    .read_request(foreign, &mut Cursor::new(&bytes))
                    .await
                    .is_err());
                assert!(codec
                    .read_response(foreign, &mut Cursor::new(&bytes))
                    .await
                    .is_err());
            }
            if protocols.media != MEDIA_PROTOCOL {
                assert!(codec
                    .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
                    .await
                    .is_err());
            }
            assert_eq!(wire.take().0, [[0; 2]; 10]);
            let unknown = frame_bytes(16);
            codec
                .read_request(
                    &StreamProtocol::new("/unknown/media"),
                    &mut Cursor::new(&unknown),
                )
                .await
                .unwrap();
            let counts = wire.take();
            assert_eq!(counts.0[WireBytes::OTHER], [16, 0]);
            assert_eq!(counts.0[4], [0, 0]);
        }
    }

    #[tokio::test]
    async fn codec_counts_exact_frames_in_all_directions_and_shares_clones() {
        use libp2p::futures::io::Cursor;
        use prost::Message;
        use request_response::Codec;
        let wire = Arc::new(WireBytes::default());
        let mut codec = ProtoCodec::<Frame, Frame>::default().with_wire(wire.clone());
        let mut cloned = codec.clone();
        for protocol in [
            NODE_PROTOCOL,
            PROFILE_PROTOCOL,
            MAILBOX_PROTOCOL,
            MAILBOX_PUSH_PROTOCOL,
            MEDIA_PROTOCOL,
            SEARCH_PROTOCOL,
            INVENTORY_PROTOCOL,
            DIGEST_PROTOCOL,
            StreamProtocol::new("/unknown"),
        ] {
            let mut incoming = frame_bytes(13);
            incoming.extend([0x10, 1]); // Unknown field: count wire bytes, not a decoded re-encoding.
            codec
                .read_request(&protocol, &mut Cursor::new(&incoming))
                .await
                .unwrap();
            cloned
                .read_response(&protocol, &mut Cursor::new(&incoming))
                .await
                .unwrap();
            let frame = Frame { data: vec![3; 7] };
            let n = frame.encoded_len();
            codec
                .write_request(&protocol, &mut Cursor::new(Vec::new()), frame.clone())
                .await
                .unwrap();
            cloned
                .write_response(&protocol, &mut Cursor::new(Vec::new()), frame)
                .await
                .unwrap();
            let mut expected = [[0; 2]; 10];
            expected[WireBytes::protocol_slot(&protocol)] =
                [2 * incoming.len() as u64, 2 * n as u64];
            assert_eq!(wire.take().0, expected);
        }
    }

    #[tokio::test]
    async fn failed_codec_io_does_not_count_and_releases_media_permits() {
        use libp2p::futures::io::Cursor;
        use request_response::Codec;
        struct BrokenRead(bool);
        impl AsyncRead for BrokenRead {
            fn poll_read(
                mut self: std::pin::Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
                buf: &mut [u8],
            ) -> std::task::Poll<io::Result<usize>> {
                if self.0 {
                    std::task::Poll::Ready(Err(io::Error::other("read failed")))
                } else {
                    self.0 = true;
                    buf[0] = 0;
                    std::task::Poll::Ready(Ok(1))
                }
            }
        }
        let wire = Arc::new(WireBytes::default());
        let mut codec = ProtoCodec::<Frame, Frame>::media().with_wire(wire.clone());
        for bytes in [
            vec![0xff],
            frame_bytes(message_limit(&MEDIA_PROTOCOL, &Protocols::public()) as usize + 1),
        ] {
            assert!(codec
                .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
                .await
                .is_err());
            assert!(codec
                .read_response(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
                .await
                .is_err());
        }
        assert!(codec
            .read_response(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
            .await
            .is_err());
        assert!(codec
            .read_request(&MEDIA_PROTOCOL, &mut BrokenRead(false))
            .await
            .is_err());
        assert!(codec
            .read_response(&MEDIA_PROTOCOL, &mut BrokenRead(false))
            .await
            .is_err());
        codec
            .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(Vec::new()))
            .await
            .unwrap();
        let cloned = codec.clone();
        assert!(cloned.permit.is_none());
        assert!(Arc::ptr_eq(&cloned.wire, &wire));
        let frame = Frame { data: vec![1; 5] };
        let mut prefix = [0; 1];
        assert!(codec
            .write_request(
                &MEDIA_PROTOCOL,
                &mut Cursor::new(&mut prefix[..]),
                frame.clone()
            )
            .await
            .is_err());
        assert_ne!(prefix[0], 0); // A partial write happened but contributes no completed frame.
        assert!(codec
            .write_response(&MEDIA_PROTOCOL, &mut Cursor::new(&mut prefix[..]), frame)
            .await
            .is_err());
        assert_eq!(codec.permits.as_ref().unwrap().available_permits(), 16);
        assert_eq!(wire.take().0, [[0; 2]; 10]);
    }

    #[tokio::test]
    async fn codec_enforces_protocol_limits_in_both_directions() {
        use libp2p::futures::io::Cursor;
        use request_response::Codec;
        let mut codec = ProtoCodec::<Frame, Frame>::default();
        for protocol in [
            NODE_PROTOCOL,
            PROFILE_PROTOCOL,
            MAILBOX_PROTOCOL,
            MAILBOX_PUSH_PROTOCOL,
            MEDIA_PROTOCOL,
        ] {
            let limit = usize::try_from(message_limit(&protocol, &Protocols::public())).unwrap();
            for size in [limit, limit + 1] {
                let bytes = frame_bytes(size);
                let request = codec
                    .read_request(&protocol, &mut Cursor::new(&bytes))
                    .await;
                let response = codec
                    .read_response(&protocol, &mut Cursor::new(&bytes))
                    .await;
                if size == limit {
                    assert!(request.is_ok(), "{protocol}: request at limit");
                    assert!(response.is_ok(), "{protocol}: response at limit");
                } else {
                    assert_eq!(request.unwrap_err().kind(), io::ErrorKind::InvalidData);
                    assert_eq!(response.unwrap_err().kind(), io::ErrorKind::InvalidData);
                }
            }
        }
        let bytes = frame_bytes(6 * 1024 * 1024);
        assert!(codec
            .read_request(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
            .await
            .is_ok());
        assert!(codec
            .read_response(&MEDIA_PROTOCOL, &mut Cursor::new(&bytes))
            .await
            .is_ok());
        assert!(codec
            .read_request(&MAILBOX_PROTOCOL, &mut Cursor::new(&bytes))
            .await
            .is_err());
    }
    use libp2p::futures::StreamExt;
    use libp2p::swarm::SwarmEvent;
    use libp2p::Multiaddr;

    #[test]
    fn pow_keypair_has_the_leading_zero_bits() {
        let peer = generate_pow_keypair(12).public().to_peer_id();
        assert!(id_has_pow(&peer, 12));
        assert!(id_has_pow(&peer, 0));
        let weak = (0..64)
            .map(|_| PeerId::random())
            .filter(|p| !id_has_pow(p, 12))
            .count();
        assert!(weak > 0);
    }

    #[test]
    fn unknown_request_variant_decodes_as_none() {
        use prost::Message;
        // Field 15 of a request is a variant from a newer client: the node answers unsupported.
        let newer = [15 << 3 | 2, 0];
        let request = proto::NodeRequest::decode(newer.as_slice()).unwrap();
        assert_eq!(request.request, None);
    }

    #[tokio::test]
    async fn client_learns_server_through_identify() {
        let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        server
            .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap())
            .unwrap();
        let addr: Multiaddr = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        // A confirmed external address switches Kademlia to server mode.
        server.add_external_address(addr.clone());

        let mut client = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
        client.dial(addr).unwrap();
        let server_id = *server.local_peer_id();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                tokio::select! {
                    _ = server.select_next_some() => {}
                    event = client.select_next_some() => {
                        route(&mut client, &event);
                        if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                            kad::Event::RoutingUpdated { peer, .. },
                        )) = event
                        {
                            if peer == server_id {
                                return;
                            }
                        }
                    }
                }
            }
        })
        .await
        .expect("client did not add the server to its routing table");
    }

    #[tokio::test]
    async fn seeds_fill_the_peer_cache_and_the_cache_joins() {
        let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let addr: Multiaddr = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        server.add_external_address(addr.clone());
        let cached: Multiaddr = format!("{addr}/p2p/{}", server.local_peer_id())
            .parse()
            .unwrap();

        let mut first = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
        join(&mut first, &[addr], &[]);
        tokio::time::timeout(Duration::from_secs(10), async {
            while known_peers(&mut first) != [cached.clone()] {
                tokio::select! {
                    _ = server.select_next_some() => {}
                    event = first.select_next_some() => route(&mut first, &event),
                }
            }
        })
        .await
        .expect("the seed did not reach the routing table");

        // The next start needs no seed: the cache alone fills the table.
        let mut next = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
        join(&mut next, &[], &known_peers(&mut first));
        assert_eq!(known_peers(&mut next), [cached]);
    }

    #[tokio::test]
    async fn one_ip_group_fills_few_routing_slots() {
        let mut swarm = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        let add = |swarm: &mut Swarm<Behaviour>, ip: String| {
            let address = format!("/ip4/{ip}/tcp/1").parse().unwrap();
            add_peer(swarm, PeerId::random(), address)
        };
        assert!((1..=5).all(|i| add(&mut swarm, format!("10.{i}.0.1"))));
        let peer = PeerId::random();
        assert!(!routed(&mut swarm, &peer));
        let address = "/ip4/10.6.0.1/tcp/1".parse().unwrap();
        assert!(add_peer(&mut swarm, peer, address));
        assert!(routed(&mut swarm, &peer));
        let admitted = (1..=30)
            .filter(|i| add(&mut swarm, format!("10.9.9.{i}")))
            .count();
        assert!((1..=MAX_GROUP_PER_TABLE).contains(&admitted), "{admitted}");
        for bucket in swarm.behaviour_mut().kad.kbuckets() {
            let same = bucket.iter().filter(|e| {
                e.node
                    .value
                    .iter()
                    .any(|a| a.to_string().starts_with("/ip4/10.9.9."))
            });
            assert!(same.count() <= MAX_GROUP_PER_BUCKET);
        }
        // One /16 in several /24s: still one peer per bucket.
        let admitted = (1..=30)
            .filter(|i| add(&mut swarm, format!("10.7.{i}.1")))
            .count();
        assert!(admitted >= 1);
        for bucket in swarm.behaviour_mut().kad.kbuckets() {
            let wide = bucket.iter().filter(|e| {
                e.node
                    .value
                    .iter()
                    .any(|a| a.to_string().starts_with("/ip4/10.7."))
            });
            assert!(wide.count() <= 1);
        }
    }

    #[tokio::test]
    async fn client_gets_node_info() {
        let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let addr: Multiaddr = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        let mut client = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
        client.add_peer_address(*server.local_peer_id(), addr);
        let request = proto::NodeRequest {
            request: Some(proto::node_request::Request::Info(proto::InfoRequest {})),
        };
        client
            .behaviour_mut()
            .node
            .send_request(server.local_peer_id(), request);

        let reply = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                tokio::select! {
                    event = server.select_next_some() => {
                        if let SwarmEvent::Behaviour(BehaviourEvent::Node(
                            request_response::Event::Message {
                                message: request_response::Message::Request { request: got, channel, .. },
                                ..
                            },
                        )) = event
                        {
                            assert_eq!(got, request);
                            let response = proto::NodeResponse {
                                status: proto::Status::Ok.into(),
                                info: Some(proto::NodeInfo { max_profile_bytes: 7, ..Default::default() }),
                                ..Default::default()
                            };
                            server.behaviour_mut().node.send_response(channel, response).unwrap();
                        }
                    }
                    event = client.select_next_some() => {
                        if let SwarmEvent::Behaviour(BehaviourEvent::Node(
                            request_response::Event::Message {
                                message: request_response::Message::Response { response, .. },
                                ..
                            },
                        )) = event
                        {
                            return response;
                        }
                    }
                }
            }
        })
        .await
        .expect("no reply");
        assert_eq!(reply.info.unwrap().max_profile_bytes, 7);
    }
}
