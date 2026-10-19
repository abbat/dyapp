//! The node protocol end to end: a `node::run` loop on loopback TCP and libp2p clients.

use dyapp_bootstrap::config::{Limits, Role};
use dyapp_bootstrap::search::SearchIndex;
use dyapp_bootstrap::service::Service;
use dyapp_bootstrap::{node, BootstrapStore, NodeConfig};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, mailbox_request, node_request, profile_request, Status};
use dyapp_p2p_net::{Behaviour, BehaviourEvent, Mode, MAX_CONTROL_MESSAGE_BYTES};
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
    start_config(|c| change(&mut c.limits)).await
}

/// Starts a node with its configured media/search stores.
async fn start_config(change: impl FnOnce(&mut NodeConfig)) -> Multiaddr {
    start_config_with_wire(change).await.0
}

async fn start_config_with_wire(
    change: impl FnOnce(&mut NodeConfig),
) -> (Multiaddr, std::sync::Arc<dyapp_p2p_net::WireBytes>) {
    start_config_in_protocols(
        change,
        std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
    )
    .await
}

async fn start_config_in_protocols(
    change: impl FnOnce(&mut NodeConfig),
    protocols: std::sync::Arc<dyapp_p2p_net::Protocols>,
) -> (Multiaddr, std::sync::Arc<dyapp_p2p_net::WireBytes>) {
    let dir = format!("/tmp/ai/test-protocol-{}", uuid::Uuid::new_v4());
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    change(&mut config);
    let keypair = Keypair::generate_ed25519();
    let peer = keypair.public().to_peer_id();
    let mut service = Service::new(BootstrapStore::new(&dir).unwrap(), config);
    service.protocols = protocols;
    let mut swarm = node::swarm_with_protocols(
        keypair,
        &service.config.limits,
        &service.config.roles,
        service.wire.clone(),
        service.protocols.clone(),
    )
    .unwrap();
    if service.config.roles.contains(&Role::Media) {
        service.media =
            Some(dyapp_bootstrap::media::MediaStore::open(&service.config.storage.dir).unwrap());
    }
    if service.config.roles.contains(&Role::Search) {
        let path = service.config.storage.search_path();
        let index = SearchIndex::open(&path, service.config.search.max_bytes).unwrap();
        service.search = Some(std::sync::Arc::new(index));
    }
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            break address;
        }
    };
    let wire = service.wire.clone();
    tokio::spawn(node::run(swarm, service));
    (addr.with(libp2p::multiaddr::Protocol::P2p(peer)), wire)
}

enum ReplicaRequest {
    Profile(SignedRecord),
    Mailbox(SignedRecord),
    Media(proto::media_request::Request),
}

impl ReplicaRequest {
    async fn send(&self, client: &mut Client) -> Status {
        let code = match self {
            Self::Profile(record) => {
                client
                    .profile(profile_request::Request::ReplicaPut(record.clone()))
                    .await
                    .status
            }
            Self::Mailbox(record) => {
                client
                    .mailbox(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                        envelopes: vec![record.clone()],
                    }))
                    .await
                    .status
            }
            Self::Media(request) => client.media(request.clone()).await.status,
        };
        status(code)
    }
}

fn replica_requests() -> [ReplicaRequest; 4] {
    let owner = Identity::generate();
    let (manifest, shards) =
        dyapp_bootstrap::replication::media_encode(&vec![1; (1 << 20) + 1]).unwrap();
    let data = vec![3];
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![
            dyapp_identity::sha256(&data).to_vec(),
            manifest.hash.clone(),
        ],
        time: chrono::Utc::now().timestamp().unsigned_abs(),
    };
    let authorization = Some(proto::media_replica_put::Authorization::Keep(
        owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
    ));
    [
        ReplicaRequest::Profile(
            Profile {
                version: 1,
                ..Default::default()
            }
            .sign(&owner),
        ),
        ReplicaRequest::Mailbox(
            owner.sign(
                Domain::Envelope,
                proto::Envelope {
                    id: vec![7; 16],
                    mailbox: vec![8; 32],
                    ciphertext: vec![9],
                }
                .encode_to_vec(),
            ),
        ),
        ReplicaRequest::Media(proto::media_request::Request::ReplicaPut(
            proto::MediaReplicaPut {
                data,
                authorization: authorization.clone(),
                ..Default::default()
            },
        )),
        ReplicaRequest::Media(proto::media_request::Request::ShardPut(
            proto::MediaShardPut {
                replica: Some(proto::MediaReplicaPut {
                    manifest: Some(manifest),
                    authorization,
                    ..Default::default()
                }),
                data: shards[0].clone(),
                index: 0,
            },
        )),
    ]
}

#[tokio::test]
async fn replica_writes_check_transport_pow_before_accepting_the_record() {
    let addr = start_config(|config| {
        config.network.id_pow_bits = 8;
        config.roles = vec![Role::Store, Role::Media];
    })
    .await;
    let weak = loop {
        let key = Keypair::generate_ed25519();
        if !dyapp_p2p_net::id_has_pow(&key.public().to_peer_id(), 8) {
            break key;
        }
    };
    let requests = replica_requests();
    let mut client = Client::connect_with_key(&addr, weak).await;
    for (i, request) in requests.iter().enumerate() {
        assert_eq!(
            request.send(&mut client).await,
            Status::Denied,
            "replica {i}"
        );
    }
    let key = dyapp_p2p_net::generate_pow_keypair(8);
    let mut node = Client::connect_with_key(&addr, key).await;
    for (i, request) in requests.iter().enumerate() {
        assert_eq!(request.send(&mut node).await, Status::Ok, "replica {i}");
    }
}

#[tokio::test]
async fn replica_peer_and_group_refusals_keep_the_connection_open() {
    let key = dyapp_p2p_net::generate_pow_keypair(8);
    let requests = replica_requests();
    for group_bucket in [false, true] {
        for (i, request) in requests.iter().enumerate() {
            let addr = start_config(|config| {
                config.network.id_pow_bits = 8;
                config.roles = vec![Role::Store, Role::Media];
                config.limits.strikes_to_ban = 1;
                if group_bucket {
                    config.limits.ip_group_requests_per_second = 1;
                } else {
                    config.limits.requests_per_second = 1;
                }
            })
            .await;
            let mut client = Client::connect_with_key(&addr, key.clone()).await;
            assert_eq!(request.send(&mut client).await, Status::Ok, "replica {i}");
            assert_eq!(
                request.send(&mut client).await,
                Status::RateLimited,
                "replica {i}, group {group_bucket}"
            );
            // One strike would ban this connection; info must still answer on the same one.
            assert_eq!(status(client.info().await.status), Status::Ok);
        }
    }
}

/// A typed client on one connection, so mailbox challenges stay valid between calls.
struct Client {
    swarm: Swarm<Behaviour>,
    server: PeerId,
    wire: std::sync::Arc<dyapp_p2p_net::WireBytes>,
}

impl Client {
    async fn connect(addr: &Multiaddr) -> Self {
        Self::connect_with_key(addr, Keypair::generate_ed25519()).await
    }

    async fn connect_with_key(addr: &Multiaddr, keypair: Keypair) -> Self {
        Self::connect_with_search(addr, keypair, false).await
    }

    async fn connect_with_search(addr: &Multiaddr, keypair: Keypair, search: bool) -> Self {
        Self::connect_in_network(
            addr,
            keypair,
            search,
            std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
        )
        .await
    }

    async fn connect_in_network(
        addr: &Multiaddr,
        keypair: Keypair,
        search: bool,
        protocols: std::sync::Arc<dyapp_p2p_net::Protocols>,
    ) -> Self {
        let wire = std::sync::Arc::new(dyapp_p2p_net::WireBytes::default());
        let mut swarm = dyapp_p2p_net::build_configured_swarm(
            keypair,
            dyapp_p2p_net::SwarmOptions {
                mode: Mode::Client,
                search,
                wire: wire.clone(),
                protocols: protocols.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        if search {
            for topic in 0..dyapp_bootstrap::search::SEARCH_TOPICS {
                swarm
                    .behaviour_mut()
                    .gossip
                    .as_mut()
                    .unwrap()
                    .subscribe(&protocols.search_topic(topic))
                    .unwrap();
            }
        }
        swarm.dial(addr.clone()).unwrap();
        loop {
            if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                swarm.select_next_some().await
            {
                return Self {
                    swarm,
                    server: peer_id,
                    wire,
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

    async fn media(&mut self, request: proto::media_request::Request) -> proto::MediaResponse {
        self.swarm.behaviour_mut().media.send_request(
            &self.server,
            proto::MediaRequest {
                request: Some(request),
            },
        );
        self.wait(|event| match event {
            BehaviourEvent::Media(event) => response(event),
            _ => None,
        })
        .await
    }

    async fn search(&mut self, request: proto::search_request::Request) -> proto::SearchResponse {
        self.swarm.behaviour_mut().search.send_request(
            &self.server,
            proto::SearchRequest {
                request: Some(request),
            },
        );
        self.wait(|event| match event {
            BehaviourEvent::Search(event) => response(event),
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

fn assert_clock_invalid(status: i32, server_time: u64, before: u64) {
    assert_eq!(Status::try_from(status).unwrap(), Status::Invalid);
    let after = chrono::Utc::now().timestamp().unsigned_abs();
    assert!(
        (before..=after).contains(&server_time),
        "{before} <= {server_time} <= {after}"
    );
}

async fn clock_node() -> Multiaddr {
    start_config(|c| {
        c.roles = vec![Role::Store, Role::Media, Role::Search];
        c.limits.requests_per_second = 1000;
        c.limits.media_requests_per_second = 100;
        c.limits.min_free_mb = 0;
    })
    .await
}

#[tokio::test]
async fn rejection_limits_survive_node_dispatch_over_tcp() {
    let address = start_config(|c| {
        c.roles = vec![Role::Store, Role::Media, Role::Search];
        c.limits.requests_per_second = 1000;
        c.limits.media_requests_per_second = 100;
        c.limits.bytes_per_second = 0;
        c.limits.min_free_mb = 0;
        c.search.max_profile_bytes = 4096;
        c.search.min_profile_pow_bits = 2;
    })
    .await;
    let mut client = Client::connect(&address).await;
    let owner = Identity::generate();
    let record = |size| SignedRecord {
        payload: vec![0; size],
        ..Default::default()
    };
    let profile = client
        .profile(profile_request::Request::Publish(record((1 << 20) + 1)))
        .await;
    assert_eq!(
        (status(profile.status), profile.max_bytes),
        (Status::TooLarge, 1 << 20)
    );
    let profile = client
        .profile(profile_request::Request::ReplicaPut(record((1 << 20) + 1)))
        .await;
    assert_eq!(
        (status(profile.status), profile.max_bytes),
        (Status::TooLarge, 1 << 20)
    );
    let mailbox = client
        .mailbox(mailbox_request::Request::Put(record(100 * 1024 + 1)))
        .await;
    assert_eq!(
        (status(mailbox.status), mailbox.max_bytes),
        (Status::TooLarge, 100 * 1024)
    );
    let mailbox = client
        .mailbox(mailbox_request::Request::ReplicaPut(proto::Envelopes {
            envelopes: vec![record(100 * 1024 + 1)],
        }))
        .await;
    assert_eq!(
        (status(mailbox.status), mailbox.max_bytes),
        (Status::TooLarge, 100 * 1024)
    );
    let media = client
        .media(proto::media_request::Request::Put(proto::MediaPut {
            owner: dyapp_identity::key_hash(&owner.public_key()).to_vec(),
            data: vec![0; (6 << 20) + 1],
        }))
        .await;
    assert_eq!(
        (status(media.status), media.max_bytes),
        (Status::TooLarge, 6 << 20)
    );
    let media = client
        .media(proto::media_request::Request::ReplicaPut(
            proto::MediaReplicaPut {
                data: vec![0; (1 << 20) + 1],
                ..Default::default()
            },
        ))
        .await;
    assert_eq!(
        (status(media.status), media.max_bytes),
        (Status::TooLarge, 1 << 20)
    );
    for request in [
        proto::media_request::Request::Keep(record(1 << 20)),
        proto::media_request::Request::Attach(record(1 << 20)),
    ] {
        let media = client.media(request).await;
        assert_eq!(
            (status(media.status), media.max_bytes),
            (Status::TooLarge, 1 << 20)
        );
    }
    let search = client
        .search(proto::search_request::Request::Publish(record(4096)))
        .await;
    assert_eq!(
        (status(search.status), search.max_bytes),
        (Status::TooLarge, 4096)
    );
    let search = client
        .search(proto::search_request::Request::Publish(
            Profile {
                version: 1,
                ..Default::default()
            }
            .sign(&owner),
        ))
        .await;
    assert_eq!(
        (
            status(search.status),
            search.required_pow_bits,
            search.server_time
        ),
        (Status::Invalid, 2, 0)
    );
    let search = client
        .search(proto::search_request::Request::Publish(
            Profile::default().sign(&owner),
        ))
        .await;
    assert_eq!(
        (status(search.status), search.required_pow_bits),
        (Status::Invalid, 0)
    );
    assert_eq!(status(client.info().await.status), Status::Ok);
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
async fn repeated_keep_uses_network_recoverability_without_a_local_copy() {
    use dyapp_bootstrap::{config::Role, media::MediaStore};
    use dyapp_p2p_net::proto::media_request::Request;
    dyapp_p2p_net::set_id_pow_bits(0);
    let mut swarms = Vec::new();
    let mut services = Vec::new();
    let mut addresses = Vec::new();
    for _ in 0..3 {
        let dir = format!("/tmp/ai/test-network-keep-{}", uuid::Uuid::new_v4());
        let mut config = NodeConfig::default();
        config.storage.dir = dir.clone().into();
        config.network.id_pow_bits = 0;
        config.roles = vec![Role::Store, Role::Media];
        config.network.storage_trust_minutes = 0;
        config.limits.min_free_mb = 0;
        let mut service = Service::new(BootstrapStore::new(&dir).unwrap(), config);
        service.media = Some(MediaStore::open(std::path::Path::new(&dir)).unwrap());
        let mut swarm = node::swarm_with_protocols(
            Keypair::generate_ed25519(),
            &service.config.limits,
            &service.config.roles,
            service.wire.clone(),
            service.protocols.clone(),
        )
        .unwrap();
        swarm
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let addr = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                break address;
            }
        };
        swarm
            .behaviour_mut()
            .kad
            .set_mode(Some(libp2p::kad::Mode::Server));
        addresses.push(addr);
        services.push(service);
        swarms.push(swarm);
    }
    let ids = swarms
        .iter()
        .map(|s| *s.local_peer_id())
        .collect::<Vec<_>>();
    let (data, hash) = (0u32..1000)
        .find_map(|n| {
            let data = n.to_le_bytes().to_vec();
            let hash = dyapp_identity::sha256(&data).to_vec();
            let holders = (0..dyapp_p2p_net::REPLICAS)
                .map(|i| {
                    let key = libp2p::kad::KBucketKey::new(dyapp_p2p_net::replica_key(&hash, i));
                    *ids.iter()
                        .min_by_key(|p| libp2p::kad::KBucketKey::from(**p).distance(&key))
                        .unwrap()
                })
                .collect::<std::collections::HashSet<_>>();
            (holders.len() == 2 && !holders.contains(&ids[0])).then_some((data, hash))
        })
        .expect("no blob keys outside the accepting node");
    let owner = Identity::generate();
    let mut keep = proto::MediaKeep {
        hashes: vec![hash.clone()],
        version: 1,
        time: chrono::Utc::now().timestamp().unsigned_abs(),
    };
    let sign = |keep: &proto::MediaKeep| owner.sign(Domain::MediaKeep, keep.encode_to_vec());
    let record = sign(&keep);
    for service in &services[1..] {
        assert_eq!(
            status(
                service
                    .store_media(&proto::MediaReplicaPut {
                        data: data.clone(),
                        authorization: Some(proto::media_replica_put::Authorization::Keep(
                            record.clone()
                        )),
                        ..Default::default()
                    })
                    .unwrap()
                    .status
            ),
            Status::Ok
        );
    }
    for (i, swarm) in swarms.iter_mut().enumerate() {
        for j in 0..3 {
            if i != j {
                dyapp_p2p_net::add_peer(swarm, ids[j], addresses[j].clone());
            }
        }
    }
    for (swarm, service) in swarms.into_iter().zip(services) {
        tokio::spawn(node::run(swarm, service));
    }
    let mut client = Client::connect(&addresses[0]).await;
    let reply = client.media(Request::Keep(record)).await;
    assert_eq!(status(reply.status), Status::Ok);
    assert!(reply.missing.is_empty());
    assert_eq!(
        status(
            client
                .media(Request::Get(proto::GetMedia { hash: hash.clone() }))
                .await
                .status
        ),
        Status::NotFound
    );
    keep.version += 1;
    let reply = client.media(Request::Keep(sign(&keep))).await;
    assert_eq!(status(reply.status), Status::Ok);
    assert!(
        reply.missing.is_empty(),
        "fresh cached network availability was replaced by a local miss"
    );
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

/// Starts a store and search node; `seed` joins it to an earlier one.
async fn start_search(seed: Option<&Multiaddr>) -> Multiaddr {
    let seed = seed.map(|addr| {
        let port = addr.iter().find_map(|p| match p {
            libp2p::multiaddr::Protocol::Tcp(port) => Some(port),
            _ => None,
        });
        format!("127.0.0.1:{}", port.unwrap())
    });
    start_config(|c| {
        c.roles = vec![Role::Store, Role::Search];
        c.seeds = seed.into_iter().collect();
        c.limits.requests_per_second = 1_000;
        c.limits.bytes_per_second = 1 << 20;
        c.search.min_profile_pow_bits = 2;
    })
    .await
}

fn age_query(age: i64) -> proto::search_request::Request {
    proto::search_request::Request::Query(proto::SearchQuery {
        conditions: vec![proto::Condition {
            field: 3,
            op: proto::Op::Eq.into(),
            ints: vec![age],
            strings: vec![],
        }],
        limit: 0,
        puzzle: None,
    })
}

#[tokio::test]
async fn search_profile_spreads_between_nodes() {
    use proto::search_request::Request;
    dyapp_p2p_net::set_id_pow_bits(0);
    let a = start_search(None).await;
    let b = start_search(Some(&a)).await;
    let mut to_a = Client::connect(&a).await;
    let mut to_b = Client::connect(&b).await;
    let mut published = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    // Gossip only reaches B once the two nodes have exchanged subscriptions, so keep publishing.
    let hit = loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no profile reached B"
        );
        let age = 20 + published.len() as i64;
        let identity = Identity::generate();
        let pow_nonce = dyapp_profile::solve(&identity.public_key(), 2, 0, 100_000).unwrap();
        let record = Profile {
            version: 1,
            age: age as u32,
            pow_nonce,
            pow_bits: 2,
            ..Profile::default()
        }
        .sign(&identity);
        let r = to_a.search(Request::Publish(record.clone())).await;
        assert_eq!(status(r.status), Status::Ok);
        published.push(record);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let r = to_b.search(age_query(age)).await;
        if let Some(hit) = r.hits.into_iter().next() {
            break hit;
        }
    };
    let get = Request::Get(proto::SearchGet {
        keys: vec![hit.key],
        puzzle: None,
    });
    let r = to_b.search(get).await;
    assert_eq!(r.records.len(), 1);
    assert!(published.contains(&r.records[0]));
}

#[path = "protocol/clock.rs"]
mod clock;

#[path = "protocol/namespaces.rs"]
mod namespaces;

#[path = "protocol/wire.rs"]
mod wire;

#[path = "protocol/search_busy.rs"]
mod search_busy;
