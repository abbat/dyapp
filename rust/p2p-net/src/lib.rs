//! libp2p node: QUIC and TCP+Noise+Yamux, Kademlia, identify, AutoNAT and the node protocol
//! (`/dyapp/node`, `/dyapp/profile`) over request-response with protobuf messages.

use libp2p::futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::identity::Keypair;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{autonat, identify, kad, noise, tcp, yamux, StreamProtocol, Swarm, SwarmBuilder};
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

/// Largest request or reply on the wire: a profile payload of up to 1 MiB plus its signature.
pub const MAX_MESSAGE_BYTES: u64 = 2 * 1024 * 1024;

pub type NodeBehaviour =
    request_response::Behaviour<ProtoCodec<proto::NodeRequest, proto::NodeResponse>>;
pub type ProfileBehaviour =
    request_response::Behaviour<ProtoCodec<proto::ProfileRequest, proto::ProfileResponse>>;

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    pub identify: identify::Behaviour,
    pub autonat: autonat::Behaviour,
    pub node: NodeBehaviour,
    pub profile: ProfileBehaviour,
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
    Ok(SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            let peer_id = key.public().to_peer_id();
            let mut kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                kad::Config::new(KAD_PROTOCOL),
            );
            let (kad_mode, support) = match mode {
                Mode::Auto => (None, ProtocolSupport::Full),
                Mode::Client => (Some(kad::Mode::Client), ProtocolSupport::Outbound),
            };
            kad.set_mode(kad_mode);
            let config = request_response::Config::default();
            Behaviour {
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
                profile: request_response::Behaviour::new([(PROFILE_PROTOCOL, support)], config),
            }
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build())
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
