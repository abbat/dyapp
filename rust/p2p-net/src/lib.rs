//! libp2p node: QUIC and TCP+Noise+Yamux over DNS, Kademlia, identify, AutoNAT and the node
//! protocol (`/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push`) over
//! request-response with protobuf messages.

use libp2p::futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{
    autonat, connection_limits, identify, kad, noise, tcp, yamux, Multiaddr, StreamProtocol, Swarm,
    SwarmBuilder,
};
use std::io;
use std::marker::PhantomData;
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

/// Replicas of every profile, envelope and signal (ADR 0009).
pub const REPLICAS: u8 = 5;

/// Kademlia lookup key of replica `i` (`0..REPLICAS`) of `key`: Kademlia hashes it with SHA-256,
/// so the replica lives on the store nodes closest to H(key ‖ i). The client writes every replica.
pub fn replica_key(key: &[u8], i: u8) -> Vec<u8> {
    [key, &[i]].concat()
}

/// Largest request or reply on the wire: a profile payload of up to 1 MiB plus its signature.
pub const MAX_MESSAGE_BYTES: u64 = 2 * 1024 * 1024;

pub type NodeBehaviour =
    request_response::Behaviour<ProtoCodec<proto::NodeRequest, proto::NodeResponse>>;
pub type ProfileBehaviour =
    request_response::Behaviour<ProtoCodec<proto::ProfileRequest, proto::ProfileResponse>>;
pub type MailboxBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MailboxRequest, proto::MailboxResponse>>;
pub type PushBehaviour =
    request_response::Behaviour<ProtoCodec<proto::MailboxPush, proto::MailboxPushResponse>>;

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub limits: connection_limits::Behaviour,
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    pub identify: identify::Behaviour,
    pub autonat: autonat::Behaviour,
    pub node: NodeBehaviour,
    pub profile: ProfileBehaviour,
    pub mailbox: MailboxBehaviour,
    /// Node to client only: a node sends, a client receives.
    pub push: PushBehaviour,
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
) -> io::Result<M> {
    let mut buf = Vec::new();
    // One byte over the limit tells an oversized message from one that is exactly at it.
    io.take(MAX_MESSAGE_BYTES + 1).read_to_end(&mut buf).await?;
    if buf.len() as u64 > MAX_MESSAGE_BYTES {
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
        _: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Req> {
        read_message(io).await
    }

    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Resp> {
        read_message(io).await
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
    )
}

/// [`build_swarm`] with connection limits and at most `max_streams` concurrent streams per
/// connection and protocol.
pub fn build_limited_swarm(
    keypair: Keypair,
    mode: Mode,
    limits: connection_limits::ConnectionLimits,
    max_streams: usize,
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
            let mut kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                kad::Config::new(KAD_PROTOCOL),
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
                    [(MAILBOX_PROTOCOL, support)],
                    config.clone(),
                ),
                push: request_response::Behaviour::new([(MAILBOX_PUSH_PROTOCOL, push)], config),
            }
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build())
}

/// Starts joining the network: adds `cached` peers (`.../p2p/<id>` addresses from
/// [`known_peers`]) to the routing table and dials every seed. A `/dnsaddr/<host>` seed expands
/// to the `dnsaddr=` TXT records of `_dnsaddr.<host>`; the dial stops at the first that answers.
/// Kademlia bootstraps on its own once the first peer is in the table.
pub fn join(swarm: &mut Swarm<Behaviour>, seeds: &[Multiaddr], cached: &[Multiaddr]) {
    for address in cached {
        if let Some(Protocol::P2p(peer)) = address.iter().last() {
            swarm
                .behaviour_mut()
                .kad
                .add_address(&peer, address.clone());
        }
    }
    for seed in seeds {
        if let Err(error) = swarm.dial(seed.clone()) {
            tracing::warn!(%seed, %error, "seed not dialed");
        }
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
    use libp2p::futures::StreamExt;
    use libp2p::swarm::SwarmEvent;
    use libp2p::Multiaddr;

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
                    _ = first.select_next_some() => {}
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
