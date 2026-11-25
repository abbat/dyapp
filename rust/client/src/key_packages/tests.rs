use super::*;
use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message};
use libp2p::swarm::SwarmEvent;
use libp2p::Multiaddr;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

fn identity(secret: u8) -> Identity {
    Identity::from_secret(&[secret; 32])
}

fn profile(owner: &Identity) -> VerifiedProfile {
    let profile = dyapp_profile::Profile {
        version: 7,
        ..Default::default()
    };
    dyapp_profile::verify(&profile.sign(owner)).unwrap()
}

fn response(status: Status, record: Option<SignedRecord>) -> proto::KeyPackageResponse {
    proto::KeyPackageResponse {
        status: status.into(),
        record,
        ..Default::default()
    }
}

async fn mock_node(
    replies: Vec<proto::KeyPackageResponse>,
) -> (
    Multiaddr,
    Arc<Mutex<Vec<key_package_request::Request>>>,
    JoinHandle<()>,
) {
    let mut server =
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&requests);
    let mut replies = VecDeque::from(replies);
    let task = tokio::spawn(async move {
        loop {
            let event = server.select_next_some().await;
            let SwarmEvent::Behaviour(BehaviourEvent::KeyPackage(Event::Message {
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            })) = event
            else {
                continue;
            };
            received.lock().unwrap().push(request.request.unwrap());
            server
                .behaviour_mut()
                .key_package
                .send_response(channel, replies.pop_front().unwrap())
                .unwrap();
        }
    });
    (address, requests, task)
}

async fn connect(address: Multiaddr) -> Node {
    Node::connect(address, Arc::new(Protocols::public()))
        .await
        .unwrap()
}

fn with_mailbox(signed: &SignedRecord, device: &Identity, mailbox: Vec<u8>) -> SignedRecord {
    let mut record = proto::KeyPackageRecord::decode(signed.payload.as_slice()).unwrap();
    record.mailbox = mailbox;
    device.sign(Domain::KeyPackage, record.encode_to_vec())
}

#[test]
fn signed_records_bind_every_field_and_use_the_publication_expiry() {
    let owner = identity(1);
    let device = identity(2);
    let before = now();
    let signed = sign_key_package(&device, &owner.peer_id(), vec![3, 4, 5], true, 9).unwrap();
    let after = now();
    let issuer = signed.verify(Domain::KeyPackage).unwrap();
    assert_eq!(unhex(&issuer).unwrap(), mailbox(&device.public_key()));
    let record = proto::KeyPackageRecord::decode(signed.payload.as_slice()).unwrap();
    assert_eq!(record.owner, unhex(&owner.peer_id()).unwrap());
    assert_eq!(record.mailbox, mailbox(&device.public_key()));
    assert_eq!(record.package, [3, 4, 5]);
    assert!(record.last_resort);
    assert_eq!(record.revision, 9);
    assert!(record.expires >= before + KEY_PACKAGE_RECORD_SECONDS);
    assert!(record.expires <= after + KEY_PACKAGE_RECORD_SECONDS);
}

#[tokio::test]
async fn publication_sends_the_signed_record_and_preserves_rejection() {
    let owner = identity(1);
    let device = identity(2);
    let signed = sign_key_package(&device, &owner.peer_id(), vec![3, 4, 5], true, 9).unwrap();
    let (address, requests, task) = mock_node(vec![
        response(Status::Ok, None),
        response(Status::Denied, None),
    ])
    .await;
    let mut node = connect(address).await;
    node.publish_key_package(&device, &owner.peer_id(), vec![3, 4, 5], true, 9)
        .await
        .unwrap();
    assert!(matches!(
        node.publish_signed_key_package(signed.clone()).await,
        Err(Error::Status(Status::Denied))
    ));
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let key_package_request::Request::Publish(published) = &requests[0] else {
        panic!("expected publication");
    };
    published.verify(Domain::KeyPackage).unwrap();
    let record = proto::KeyPackageRecord::decode(published.payload.as_slice()).unwrap();
    assert_eq!(record.owner, unhex(&owner.peer_id()).unwrap());
    assert_eq!(record.mailbox, mailbox(&device.public_key()));
    assert_eq!(record.package, [3, 4, 5]);
    assert!(record.last_resort);
    assert_eq!(record.revision, 9);
    let key_package_request::Request::Publish(published) = &requests[1] else {
        panic!("expected publication");
    };
    assert_eq!(published.encode_to_vec(), signed.encode_to_vec());
    drop(requests);
    task.abort();
}

#[tokio::test]
async fn fetching_retries_slots_and_rejects_missing_or_foreign_records() {
    let owner = identity(1);
    let device = identity(2);
    let other = identity(3);
    let owner_profile = profile(&owner);
    let address = mailbox(&device.public_key()).to_vec();
    let signed = sign_key_package(&device, &owner.peer_id(), vec![3, 4, 5], false, 0).unwrap();
    let wrong_issuer = with_mailbox(
        &sign_key_package(&other, &owner.peer_id(), vec![6], false, 0).unwrap(),
        &other,
        address.clone(),
    );
    let wrong_record = with_mailbox(&signed, &device, mailbox(&other.public_key()).to_vec());
    let (server_address, requests, task) = mock_node(vec![
        response(Status::NotFound, None),
        response(Status::Ok, Some(signed)),
        response(Status::Ok, Some(wrong_issuer)),
        response(Status::Ok, Some(wrong_record)),
        response(Status::Ok, None),
    ])
    .await;
    let mut node = connect(server_address).await;
    assert_eq!(
        node.fetch_key_package(&owner_profile, &address)
            .await
            .unwrap(),
        [3, 4, 5]
    );
    for _ in 0..2 {
        assert!(matches!(
            node.fetch_key_package(&owner_profile, &address).await,
            Err(Error::Reply("KeyPackage of another device"))
        ));
    }
    assert!(matches!(
        node.fetch_key_package(&owner_profile, &address).await,
        Err(Error::Reply("no record"))
    ));
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    for (slot, request) in requests.iter().take(2).enumerate() {
        let key_package_request::Request::Fetch(fetch) = request else {
            panic!("expected fetch");
        };
        assert_eq!(fetch.owner, unhex(&owner.peer_id()).unwrap());
        assert_eq!(fetch.mailbox, address);
        assert_eq!(fetch.profile_version, 7);
        assert_eq!(fetch.slot, u32::try_from(slot).unwrap());
    }
    drop(requests);
    task.abort();
}
