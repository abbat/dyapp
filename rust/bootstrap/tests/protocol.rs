//! The node protocol end to end: a `node::run` loop on loopback TCP and libp2p clients.

use dyapp_bootstrap::config::Limits;
use dyapp_bootstrap::service::Service;
use dyapp_bootstrap::{node, BootstrapStore, NodeConfig};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, mailbox_request, node_request, profile_request, Status};
use dyapp_p2p_net::{build_swarm, Behaviour, BehaviourEvent, Mode, MAX_CONTROL_MESSAGE_BYTES};
use dyapp_profile::Profile;
use libp2p::futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, StreamExt};
use libp2p::identity::Keypair;
use libp2p::request_response::{self, Event, Message, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{noise, tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use prost::Message as _;
use std::io;
use std::time::Duration;

/// Starts a node with its own store and returns its address.
async fn start(requests_per_second: u32) -> Multiaddr {
    start_with(|l| l.requests_per_second = requests_per_second).await
}

async fn start_with(change: impl FnOnce(&mut Limits)) -> Multiaddr {
    let dir = format!("/tmp/ai/test-protocol-{}", uuid::Uuid::new_v4());
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone().into();
    change(&mut config.limits);
    let mut swarm =
        node::swarm(Keypair::generate_ed25519(), &config.limits, &config.roles).unwrap();
    let service = Service::new(BootstrapStore::new(&dir).unwrap(), config);
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            break address;
        }
    };
    tokio::spawn(node::run(swarm, service));
    addr
}

/// A typed client on one connection, so mailbox challenges stay valid between calls.
struct Client {
    swarm: Swarm<Behaviour>,
    server: PeerId,
}

impl Client {
    async fn connect(addr: &Multiaddr) -> Self {
        let mut swarm = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
        swarm.dial(addr.clone()).unwrap();
        loop {
            if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                swarm.select_next_some().await
            {
                return Self {
                    swarm,
                    server: peer_id,
                };
            }
        }
    }

    async fn wait<R>(&mut self, pick: impl Fn(BehaviourEvent) -> Option<R>) -> R {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let SwarmEvent::Behaviour(event) = self.swarm.select_next_some().await {
                    if let Some(reply) = pick(event) {
                        return reply;
                    }
                }
            }
        })
        .await
        .expect("no reply")
    }

    async fn info(&mut self) -> proto::NodeResponse {
        let request = proto::NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        };
        self.swarm
            .behaviour_mut()
            .node
            .send_request(&self.server, request);
        self.wait(|event| match event {
            BehaviourEvent::Node(event) => response(event),
            _ => None,
        })
        .await
    }

    async fn profile(&mut self, request: profile_request::Request) -> proto::ProfileResponse {
        let request = proto::ProfileRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .profile
            .send_request(&self.server, request);
        self.wait(|event| match event {
            BehaviourEvent::Profile(event) => response(event),
            _ => None,
        })
        .await
    }

    async fn mailbox(&mut self, request: mailbox_request::Request) -> proto::MailboxResponse {
        let request = proto::MailboxRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .mailbox
            .send_request(&self.server, request);
        self.wait(|event| match event {
            BehaviourEvent::Mailbox(event) => response(event),
            _ => None,
        })
        .await
    }

    async fn challenge(&mut self) -> Vec<u8> {
        let challenge = mailbox_request::Request::Challenge(proto::ChallengeRequest {});
        self.mailbox(challenge).await.nonce
    }
}

fn response<Req, Resp>(event: Event<Req, Resp>) -> Option<Resp> {
    match event {
        Event::Message {
            message: Message::Response { response, .. },
            ..
        } => Some(response),
        Event::OutboundFailure { error, .. } => panic!("request failed: {error}"),
        _ => None,
    }
}

fn status(value: i32) -> Status {
    Status::try_from(value).unwrap()
}

fn fetch(device: &Identity, nonce: Vec<u8>) -> mailbox_request::Request {
    let fetch = proto::Fetch {
        nonce,
        ..proto::Fetch::default()
    };
    mailbox_request::Request::Fetch(device.sign(Domain::MailboxFetch, fetch.encode_to_vec()))
}

/// Sends raw bytes on `protocol` from a fresh connection: `None` when the node closes the stream
/// without a reply (a typed client decodes that as `STATUS_UNSPECIFIED`) or the request fails.
async fn raw(addr: &Multiaddr, protocol: StreamProtocol, bytes: Vec<u8>) -> Option<Vec<u8>> {
    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|_| {
            request_response::Behaviour::<RawCodec>::new(
                [(protocol, ProtocolSupport::Outbound)],
                request_response::Config::default(),
            )
        })
        .unwrap()
        .build();
    swarm.dial(addr.clone()).unwrap();
    let server = loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => break peer_id,
            SwarmEvent::OutgoingConnectionError { .. } => return None,
            _ => {}
        }
    };
    swarm.behaviour_mut().send_request(&server, bytes);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match swarm.select_next_some().await {
                SwarmEvent::Behaviour(Event::Message {
                    message: Message::Response { response, .. },
                    ..
                }) => return (!response.is_empty()).then_some(response),
                SwarmEvent::Behaviour(Event::OutboundFailure { .. }) => return None,
                _ => {}
            }
        }
    })
    .await
    .expect("no reply or failure")
}

/// Writes request bytes as they are and reads the reply undecoded.
#[derive(Clone, Default)]
struct RawCodec;

impl request_response::Codec for RawCodec {
    type Protocol = StreamProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        _: &mut T,
    ) -> io::Result<Vec<u8>> {
        unreachable!("outbound only")
    }

    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        Ok(buf)
    }

    async fn write_request<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        request: Vec<u8>,
    ) -> io::Result<()> {
        io.write_all(&request).await
    }

    async fn write_response<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        _: &mut T,
        _: Vec<u8>,
    ) -> io::Result<()> {
        unreachable!("outbound only")
    }
}

#[tokio::test]
async fn every_request_type() {
    let addr = start(100).await;
    let mut client = Client::connect(&addr).await;

    let info = client.info().await;
    assert_eq!(status(info.status), Status::Ok);
    let info = info.info.unwrap();
    assert_eq!(info.roles, [proto::Role::Store as i32]);
    assert_eq!(info.max_message_bytes, 100 * 1024);

    let identity = Identity::generate();
    let record = Profile {
        version: 1,
        age: 31,
        ..Profile::default()
    }
    .sign(&identity);
    let published = client
        .profile(profile_request::Request::Publish(record.clone()))
        .await;
    assert_eq!(status(published.status), Status::Ok);
    let peer_id = dyapp_identity::key_hash(&identity.public_key()).to_vec();
    let got = client
        .profile(profile_request::Request::Get(proto::GetProfile { peer_id }))
        .await;
    assert_eq!((status(got.status), got.record), (Status::Ok, Some(record)));

    let device = Identity::generate();
    let envelope = proto::Envelope {
        id: vec![7; 16],
        mailbox: dyapp_identity::key_hash(&device.public_key()).to_vec(),
        ciphertext: b"hello".to_vec(),
    };
    let put: SignedRecord = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
    let reply = client
        .mailbox(mailbox_request::Request::Put(put.clone()))
        .await;
    assert_eq!(status(reply.status), Status::Ok);

    let nonce = client.challenge().await;
    let reply = client.mailbox(fetch(&device, nonce)).await;
    assert_eq!(
        (status(reply.status), reply.envelopes),
        (Status::Ok, vec![put])
    );

    let nonce = client.challenge().await;
    let ack = proto::Ack {
        nonce,
        ids: vec![envelope.id],
    };
    let ack = device.sign(Domain::MailboxAck, ack.encode_to_vec());
    let reply = client.mailbox(mailbox_request::Request::Ack(ack)).await;
    assert_eq!(status(reply.status), Status::Ok);
    let nonce = client.challenge().await;
    assert!(client
        .mailbox(fetch(&device, nonce))
        .await
        .envelopes
        .is_empty());
}

#[tokio::test]
async fn challenge_is_bound_to_its_connection() {
    let addr = start(100).await;
    let mut first = Client::connect(&addr).await;
    let mut second = Client::connect(&addr).await;
    let device = Identity::generate();
    let nonce = first.challenge().await;
    second.challenge().await;
    let reply = second.mailbox(fetch(&device, nonce.clone())).await;
    assert_eq!(status(reply.status), Status::Denied);
    let reply = first.mailbox(fetch(&device, nonce)).await;
    assert_eq!(status(reply.status), Status::Ok);
}

#[tokio::test]
async fn bad_input_fails_only_its_request() {
    let addr = start(100).await;
    let profile = dyapp_p2p_net::PROFILE_PROTOCOL;
    // Not protobuf: field 1 with a length running past the end.
    assert_eq!(raw(&addr, profile.clone(), vec![0x0a, 0x7f, 1]).await, None);
    let oversized = vec![0; usize::try_from(MAX_CONTROL_MESSAGE_BYTES).unwrap() + 1];
    assert_eq!(
        raw(&addr, dyapp_p2p_net::MAILBOX_PROTOCOL, oversized).await,
        None
    );
    // Field 15 is a request variant from a newer client.
    let reply = raw(&addr, profile, vec![15 << 3 | 2, 0]).await.unwrap();
    let reply = proto::ProfileResponse::decode(reply.as_slice()).unwrap();
    assert_eq!(status(reply.status), Status::Unsupported);
    // An empty mailbox request decodes as no variant too.
    let reply = raw(&addr, dyapp_p2p_net::MAILBOX_PROTOCOL, vec![])
        .await
        .unwrap();
    let reply = proto::MailboxResponse::decode(reply.as_slice()).unwrap();
    assert_eq!(status(reply.status), Status::Unsupported);

    let mut client = Client::connect(&addr).await;
    assert_eq!(status(client.info().await.status), Status::Ok);
}

#[tokio::test]
async fn requests_over_the_limit_are_refused() {
    let addr = start(2).await;
    let mut client = Client::connect(&addr).await;
    let get = || {
        profile_request::Request::Get(proto::GetProfile {
            peer_id: vec![0; 32],
        })
    };
    for _ in 0..2 {
        assert_eq!(status(client.profile(get()).await.status), Status::NotFound);
    }
    assert_eq!(
        status(client.profile(get()).await.status),
        Status::RateLimited
    );
    // Node info is not limited.
    assert_eq!(status(client.info().await.status), Status::Ok);
}

#[tokio::test]
async fn connections_over_the_limit_are_refused() {
    let addr = start_with(|l| l.max_connections = 1).await;
    let mut first = Client::connect(&addr).await;
    let info = proto::NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    assert_eq!(
        raw(&addr, dyapp_p2p_net::NODE_PROTOCOL, info.encode_to_vec()).await,
        None
    );
    assert_eq!(status(first.info().await.status), Status::Ok);
}

#[tokio::test]
async fn connections_over_the_memory_limit_are_refused() {
    // The test process alone uses more than 1 MiB.
    let addr = start_with(|l| l.max_memory_mb = 1).await;
    let info = proto::NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    assert_eq!(
        raw(&addr, dyapp_p2p_net::NODE_PROTOCOL, info.encode_to_vec()).await,
        None
    );
}
