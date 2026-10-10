//! libp2p node: QUIC and TCP+Noise+Yamux over DNS, Kademlia, identify, AutoNAT and the node
//! protocol (`/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push`,
//! `/dyapp/media`) over request-response with protobuf messages.

use libp2p::futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{
    autonat, connection_limits, identify, kad, memory_connection_limits, noise, tcp, yamux,
    Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use std::io;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

/// Node protocol messages, generated from proto/node.proto.
pub mod proto {
    #![allow(clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/dyapp.node.rs"));
}

pub const KAD_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/kad");
pub const IDENTIFY_PROTOCOL: &str = "/dyapp";
pub const NODE_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/node");
pub const PROFILE_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/profile");
pub const MAILBOX_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/mailbox");
pub const MAILBOX_PUSH_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/mailbox-push");
pub const MEDIA_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/media");
/// Kademlia provider key under which nodes with the turn role announce their relay.
pub const TURN_KEY: &[u8] = b"/dyapp/turn";

/// Replicas of every profile, envelope and signal (ADR 0009).
pub const REPLICAS: u8 = 5;

/// Kademlia lookup key of replica `i` (`0..REPLICAS`) of `key`: Kademlia hashes it with SHA-256,
/// so the replica lives on the store nodes closest to H(key ‖ i). The client writes every replica.
pub fn replica_key(key: &[u8], i: u8) -> Vec<u8> {
    [key, &[i]].concat()
}

/// Non-media frame limit, including protobuf and signature overhead.
pub const MAX_CONTROL_MESSAGE_BYTES: u64 = 1024 * 1024 + 64 * 1024;
/// Media frame limit, including protobuf and signature overhead.
pub const MAX_MEDIA_MESSAGE_BYTES: u64 = 6 * 1024 * 1024 + 64 * 1024;

fn message_limit(protocol: &StreamProtocol) -> u64 {
    if protocol == &MEDIA_PROTOCOL {
        MAX_MEDIA_MESSAGE_BYTES
    } else {
        MAX_CONTROL_MESSAGE_BYTES
    }
}

pub type NodeBehaviour =
    request_response::Behaviour<ProtoCodec<proto::NodeRequest, proto::NodeResponse>>;
pub type ProfileBehaviour =
    request_response::Behaviour<ProtoCodec<proto::ProfileRequest, proto::ProfileResponse>>;
pub type MailboxBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MailboxRequest, proto::MailboxResponse>>;
pub type PushBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MailboxPush, proto::MailboxPushResponse>>;
pub type MediaBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MediaRequest, proto::MediaResponse>>;

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub limits: connection_limits::Behaviour,
    pub memory: memory_connection_limits::Behaviour,
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    pub identify: identify::Behaviour,
    pub autonat: autonat::Behaviour,
    pub node: NodeBehaviour,
    pub profile: ProfileBehaviour,
    pub mailbox: MailboxBehaviour,
    /// Node to client only: a node sends, a client receives.
    pub push: PushBehaviour,
    pub media: MediaBehaviour,
}

/// One protobuf message per stream; the writer closes the stream after it.
pub struct ProtoCodec<Req, Resp>(PhantomData<fn() -> (Req, Resp)>);

impl<Req, Resp> Default for ProtoCodec<Req, Resp> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Req, Resp> Clone for ProtoCodec<Req, Resp> {
    fn clone(&self) -> Self {
        Self::default()
    }
}

async fn read_message<M: prost::Message + Default, T: AsyncRead + Unpin + Send>(
    io: &mut T,
    limit: u64,
) -> io::Result<M> {
    let mut buf = Vec::new();
    // One byte over the limit tells an oversized message from one that is exactly at it.
    io.take(limit + 1).read_to_end(&mut buf).await?;
    if buf.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    M::decode(buf.as_slice()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

async fn write_message<M: prost::Message, T: AsyncWrite + Unpin + Send>(
    io: &mut T,
    message: M,
) -> io::Result<()> {
    io.write_all(&message.encode_to_vec()).await
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
        read_message(io, message_limit(protocol)).await
    }

    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        protocol: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Resp> {
        read_message(io, message_limit(protocol)).await
    }

    async fn write_request<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        request: Req,
    ) -> io::Result<()> {
        write_message(io, request).await
    }

    async fn write_response<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        response: Resp,
    ) -> io::Result<()> {
        write_message(io, response).await
    }
}

/// Kademlia role of the node.
pub enum Mode {
    /// DHT server once AutoNAT or the operator confirms an external address.
    Auto,
    /// Never answers DHT queries: mobile and other short-lived clients.
    Client,
}

pub fn build_swarm(keypair: Keypair, mode: Mode) -> anyhow::Result<Swarm<Behaviour>> {
    build_limited_swarm(
        keypair,
        mode,
        connection_limits::ConnectionLimits::default(),
        100,
        usize::MAX,
    )
}

/// [`build_swarm`] with connection limits and at most `max_streams` concurrent streams per
/// connection and protocol; new connections are refused while the process uses more than
/// `max_memory_bytes` of physical memory.
pub fn build_limited_swarm(
    keypair: Keypair,
    mode: Mode,
    limits: connection_limits::ConnectionLimits,
    max_streams: usize,
    max_memory_bytes: usize,
) -> anyhow::Result<Swarm<Behaviour>> {
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
            let mut kad_config = kad::Config::new(KAD_PROTOCOL);
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
            let config =
                request_response::Config::default().with_max_concurrent_streams(max_streams);
            Behaviour {
                limits: connection_limits::Behaviour::new(limits),
                memory: memory_connection_limits::Behaviour::with_max_bytes(max_memory_bytes),
                kad,
                identify: identify::Behaviour::new(identify::Config::new(
                    IDENTIFY_PROTOCOL.into(),
                    key.public(),
                )),
                autonat: autonat::Behaviour::new(peer_id, autonat::Config::default()),
                node: request_response::Behaviour::new(
                    [(NODE_PROTOCOL, support.clone())],
                    config.clone(),
                ),
                profile: request_response::Behaviour::new(
                    [(PROFILE_PROTOCOL, support.clone())],
                    config.clone(),
                ),
                mailbox: request_response::Behaviour::new(
                    [(MAILBOX_PROTOCOL, support.clone())],
                    config.clone(),
                ),
                push: request_response::Behaviour::new(
                    [(MAILBOX_PUSH_PROTOCOL, push)],
                    config.clone(),
                ),
                media: request_response::Behaviour::new([(MEDIA_PROTOCOL, support)], config),
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
pub const MAX_GROUP_PER_TABLE: usize = 10;

static DISTINCT_GROUPS: AtomicBool = AtomicBool::new(true);

/// Whether a k-bucket takes at most one peer per wide group (/16, IPv6 /32, or one DNS name), so
/// the peers Kademlia dials for one key range sit in distinct networks (ADR 0008). On by
/// default; a node on a trusted network, where every peer shares a /16, turns it off.
pub fn set_distinct_groups(on: bool) {
    DISTINCT_GROUPS.store(on, Ordering::Relaxed);
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
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, prost::Message)]
    struct Frame {
        #[prost(bytes = "vec", tag = "1")]
        data: Vec<u8>,
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
            let limit = usize::try_from(message_limit(&protocol)).unwrap();
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
