use super::*;
use crate::chat::{self, Body};
use crate::store::Store;
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_messaging::mls::Device;
use dyapp_p2p_net::{build_configured_swarm, proto, BehaviourEvent, Protocols, SwarmOptions};
use dyapp_profile::Profile;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event as RequestEvent, Message};
use libp2p::swarm::SwarmEvent;
use libp2p::Multiaddr;
use prost::Message as _;
use std::path::PathBuf;
use std::sync::Arc;

struct Fixture {
    session: Session,
    bob_id: Identity,
    bob_device: Identity,
    profiles: [dyapp_profile::VerifiedProfile; 2],
    welcome: Vec<u8>,
    text: Vec<u8>,
    path: PathBuf,
    _test_dir: dyapp_test_support::TestDir,
}

fn fixture() -> Fixture {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let path = test_dir.path().join("client.db");
    let alice_id = Identity::try_generate().unwrap();
    let alice = Device::new(&alice_id, Identity::try_generate().unwrap().public_key(), 1).unwrap();
    let bob_id = Identity::try_generate().unwrap();
    let bob_device = Identity::try_generate().unwrap();
    let bob = Device::new(&bob_id, bob_device.public_key(), 1).unwrap();
    let profiles = [&alice_id, &bob_id].map(|identity| {
        let device = if identity.peer_id() == alice_id.peer_id() {
            &alice
        } else {
            &bob
        };
        dyapp_profile::verify(
            &Profile {
                version: 1,
                devices: vec![device.certificate().clone()],
                ..Default::default()
            }
            .sign(identity),
        )
        .unwrap()
    });
    let (mut group, welcome) = alice
        .create_group(&[bob.key_package(false).unwrap()], &profiles)
        .unwrap();
    let text = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([3; 16], b"held by the second replica".to_vec()),
    )
    .unwrap()[0]
        .mls
        .clone();
    let mut store = Store::open(&path, &bob_id).unwrap();
    store
        .stage_outbound(&bob.seal(&[5; 32]).unwrap(), &[])
        .unwrap();
    Fixture {
        session: Session::open(store, &[5; 32]).unwrap(),
        bob_id,
        bob_device,
        profiles,
        welcome,
        text,
        path,
        _test_dir: test_dir,
    }
}

struct Server {
    address: Multiaddr,
    ack_status: Arc<std::sync::atomic::AtomicI32>,
    acknowledgements: Arc<std::sync::atomic::AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

fn reply(
    request: proto::MailboxRequest,
    envelopes: &[SignedRecord],
    more: bool,
    ack_status: &std::sync::atomic::AtomicI32,
    acknowledgements: &std::sync::atomic::AtomicUsize,
) -> proto::MailboxResponse {
    let mut response = proto::MailboxResponse {
        status: proto::Status::Ok.into(),
        ..Default::default()
    };
    match request.request.unwrap() {
        proto::mailbox_request::Request::Challenge(_) => response.nonce = vec![7; 32],
        proto::mailbox_request::Request::Fetch(record) => {
            record.verify(Domain::MailboxFetch).unwrap();
            response.envelopes = envelopes.to_vec();
            response.more = more;
            if more {
                response.next_cursor = vec![1; 10];
            }
        }
        proto::mailbox_request::Request::Ack(record) => {
            record.verify(Domain::MailboxAck).unwrap();
            acknowledgements.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            response.status = ack_status.load(std::sync::atomic::Ordering::SeqCst);
        }
        proto::mailbox_request::Request::Put(_) => {}
        _ => panic!("unexpected mailbox request"),
    }
    response
}

impl Server {
    async fn new(envelopes: Vec<SignedRecord>, more: bool) -> Self {
        let mut swarm =
            build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap();
        swarm
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                break address;
            }
        };
        let envelopes = Arc::new(envelopes);
        let served = Arc::clone(&envelopes);
        let ack_status = Arc::new(std::sync::atomic::AtomicI32::new(proto::Status::Ok.into()));
        let ack = Arc::clone(&ack_status);
        let acknowledgements = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counts = Arc::clone(&acknowledgements);
        let task = tokio::spawn(async move {
            loop {
                let event = swarm.select_next_some().await;
                let SwarmEvent::Behaviour(BehaviourEvent::Mailbox(RequestEvent::Message {
                    message:
                        Message::Request {
                            request, channel, ..
                        },
                    ..
                })) = event
                else {
                    continue;
                };
                let response = reply(request, &served, more, &ack, &counts);
                swarm
                    .behaviour_mut()
                    .mailbox
                    .send_response(channel, response)
                    .unwrap();
            }
        });
        Self {
            address,
            ack_status,
            acknowledgements,
            task,
        }
    }

    async fn node(&self) -> Node {
        Node::connect(self.address.clone(), Arc::new(Protocols::public()))
            .await
            .unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn envelope(device: &Identity, id: u8, ciphertext: Vec<u8>) -> SignedRecord {
    Identity::try_generate().unwrap().sign(
        Domain::Envelope,
        proto::Envelope {
            id: vec![id; 16],
            mailbox: crate::mailbox(&device.public_key()).to_vec(),
            ciphertext,
            ..Default::default()
        }
        .encode_to_vec(),
    )
}

async fn cross(
    fixture: &mut Fixture,
    first: &mut Node,
    second: &mut Node,
) -> Result<Vec<Event>, Error> {
    fixture
        .session
        .cross_check(
            first,
            second,
            &fixture.bob_device,
            &fixture.profiles,
            &[5; 32],
        )
        .await
}

#[tokio::test]
async fn complete_withholding_is_scored_and_the_union_is_committed_and_acked() {
    let mut fixture = fixture();
    let welcome = envelope(&fixture.bob_device, 1, fixture.welcome.clone());
    let text = envelope(&fixture.bob_device, 2, fixture.text.clone());
    let first = Server::new(vec![welcome.clone()], false).await;
    let second = Server::new(vec![welcome, text], false).await;
    let mut first_node = first.node().await;
    let mut second_node = second.node().await;
    let events = cross(&mut fixture, &mut first_node, &mut second_node)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(fixture.session.store.pending_acks().unwrap().len(), 0);
    assert_eq!(
        first
            .acknowledgements
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        second
            .acknowledgements
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        fixture
            .session
            .scores
            .lock()
            .await
            .penalty(first_node.peer(), crate::now()),
        1
    );
    assert_eq!(
        fixture
            .session
            .scores
            .lock()
            .await
            .penalty(second_node.peer(), crate::now()),
        0
    );
}

#[tokio::test]
async fn partial_first_page_is_not_scored_for_omission() {
    let mut fixture = fixture();
    let welcome = envelope(&fixture.bob_device, 1, fixture.welcome.clone());
    let text = envelope(&fixture.bob_device, 2, fixture.text.clone());
    let first = Server::new(vec![welcome.clone()], true).await;
    let second = Server::new(vec![welcome, text], false).await;
    let mut first_node = first.node().await;
    let mut second_node = second.node().await;
    cross(&mut fixture, &mut first_node, &mut second_node)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .session
            .scores
            .lock()
            .await
            .penalty(first_node.peer(), crate::now()),
        0
    );
}

#[tokio::test]
async fn failed_second_ack_and_restart_keep_ids_pending_without_duplicate_events() {
    let mut fixture = fixture();
    let welcome = envelope(&fixture.bob_device, 1, fixture.welcome.clone());
    let text = envelope(&fixture.bob_device, 2, fixture.text.clone());
    let first = Server::new(vec![welcome.clone()], false).await;
    let second = Server::new(vec![welcome, text], false).await;
    second.ack_status.store(
        proto::Status::Denied.into(),
        std::sync::atomic::Ordering::SeqCst,
    );
    let mut first_node = first.node().await;
    let mut second_node = second.node().await;
    assert!(cross(&mut fixture, &mut first_node, &mut second_node)
        .await
        .is_err());
    assert_eq!(fixture.session.store.pending_acks().unwrap().len(), 2);
    drop(fixture.session);
    fixture.session = Session::open(
        Store::open(&fixture.path, &fixture.bob_id).unwrap(),
        &[5; 32],
    )
    .unwrap();
    assert_eq!(fixture.session.store.pending_acks().unwrap().len(), 2);
    second.ack_status.store(
        proto::Status::Ok.into(),
        std::sync::atomic::Ordering::SeqCst,
    );
    let events = cross(&mut fixture, &mut first_node, &mut second_node)
        .await
        .unwrap();
    assert_eq!(events.len(), 0);
    assert_eq!(fixture.session.store.pending_acks().unwrap().len(), 0);
    assert_eq!(
        first
            .acknowledgements
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    assert_eq!(
        second
            .acknowledgements
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
}
