//! libp2p node: QUIC and TCP+Noise+Yamux over DNS, Kademlia, identify, AutoNAT and the node
//! protocol (`/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push`,
//! `/dyapp/media`) over request-response with protobuf messages. A search node adds
//! `/dyapp/search-kad`, gossipsub, `/dyapp/search`, `/dyapp/inventory` and `/dyapp/digest`.

use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::upgrade;
use libp2p::futures::future::Either;
use libp2p::futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{
    autonat, connection_limits, gossipsub, identify, kad, memory_connection_limits, noise, quic,
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder, Transport,
};
use std::error::Error;
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
pub mod hints;
pub use ipnet::IpNet;
mod incoming_groups;
pub use incoming_groups::{IncomingGroupGuard, IncomingGroupLimits};
mod pow_gate;
pub use pow_gate::PowGate;
mod protocols;
pub use protocols::Protocols;
pub mod search_results;
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
/// Device KeyPackage publication and allocation.
pub const KEY_PACKAGE_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/keypackage");
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
/// Search bucket digests exchanged between search nodes.
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
/// Request-response behaviour of [`KEY_PACKAGE_PROTOCOL`].
pub type KeyPackageBehaviour =
    request_response::Behaviour<ProtoCodec<proto::KeyPackageRequest, proto::KeyPackageResponse>>;
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
/// Request-response behaviour serving search bucket digests.
pub type DigestBehaviour =
    request_response::Behaviour<ProtoCodec<proto::DigestRequest, proto::DigestResponse>>;

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
    /// Outbound dial guard of a node: wraps the transport below DNS and adds a [`PowGate`].
    /// Clients pass `None` and dial unguarded.
    pub dial_guard: Option<DialGuard>,
    /// Outgoing deny ranges, checked below DNS before the dial guard; empty allows all.
    pub dial_filter: DialFilter,
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
            dial_guard: None,
            dial_filter: DialFilter::default(),
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
            ..SwarmOptions::default()
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
        dial_guard,
        dial_filter,
    } = options;
    let pow_gate = dial_guard.clone().map(PowGate::new);
    Ok(SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_other_transport(|key| -> Result<_, Box<dyn Error + Send + Sync>> {
            let tcp = tcp::tokio::Transport::new(tcp::Config::default())
                .upgrade(upgrade::Version::V1Lazy)
                .authenticate(noise::Config::new(key)?)
                .multiplex(yamux::Config::default());
            let combined = quic::tokio::Transport::new(quic::Config::new(key))
                .or_transport(tcp)
                .map(|output, _| match output {
                    Either::Left((peer, muxer)) => (peer, StreamMuxerBox::new(muxer)),
                    Either::Right((peer, muxer)) => (peer, StreamMuxerBox::new(muxer)),
                });
            // Below DNS, so the filter and the guard see every resolved IP, not the name.
            let guarded = match dial_guard {
                Some(guard) => GuardDial::new(combined, guard).boxed(),
                None => combined.boxed(),
            };
            Ok(DenyDial::new(guarded, dial_filter).boxed())
        })
        .map_err(|error| anyhow::anyhow!("transport: {error}"))?
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
            let mut search_config = kad::Config::new(protocols.search_kad.clone());
            search_config.set_kbucket_inserts(kad::BucketInserts::Manual);
            let mut search_kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                search_config,
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
                pow_gate: pow_gate.into(),
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
                key_package: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.key_package.clone(), support.clone())],
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
                    [(protocols.inventory.clone(), search_support.clone())],
                    config.clone(),
                ),
                digest: request_response::Behaviour::with_codec(
                    ProtoCodec::default()
                        .with_wire(wire.clone())
                        .with_protocols(protocols.clone()),
                    [(protocols.digest.clone(), search_support)],
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
    admit(&mut swarm.behaviour_mut().kad, peer, address)
}

/// [`add_peer`] for the search Kademlia, which links search nodes: the same proof of work and
/// IP-group limits, counted in its own table.
pub fn add_search_peer(swarm: &mut Swarm<Behaviour>, peer: PeerId, address: Multiaddr) -> bool {
    admit(&mut swarm.behaviour_mut().search_kad, peer, address)
}

fn admit(
    kad: &mut kad::Behaviour<kad::store::MemoryStore>,
    peer: PeerId,
    address: Multiaddr,
) -> bool {
    if !id_has_pow(&peer, id_pow_bits()) {
        tracing::debug!(%peer, "peer ID without proof of work not routed");
        return false;
    }
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

/// Feeds a peer either Kademlia found routable (a dialled peer, a lookup result) through
/// [`add_peer`] or [`add_search_peer`]; call it on every swarm event.
pub fn route(swarm: &mut Swarm<Behaviour>, event: &SwarmEvent<BehaviourEvent>) {
    match event {
        SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::RoutablePeer { peer, address })) => {
            add_peer(swarm, *peer, address.clone());
        }
        SwarmEvent::Behaviour(BehaviourEvent::SearchKad(kad::Event::RoutablePeer {
            peer,
            address,
        })) => {
            add_search_peer(swarm, *peer, address.clone());
        }
        _ => {}
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
#[path = "lib/tests.rs"]
mod tests;
