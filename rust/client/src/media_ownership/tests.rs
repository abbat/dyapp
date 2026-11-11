use super::{parse_replaced, verify_missing, Replaced, State};
use crate::{Error, Node};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, media_request, Status};
use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message as RequestMessage};
use libp2p::swarm::SwarmEvent;
use prost::Message;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn owner() -> Identity {
    Identity::from_secret(&[49; 32])
}

fn signed(owner: &Identity, version: u64) -> SignedRecord {
    let keep = proto::MediaKeepIntent {
        version,
        hashes: vec![vec![7; 32]],
        time: 10,
        expected: Some(proto::MediaKeepCondition {
            owner_present: version > 1,
            version: version - 1,
            record_hash: Vec::new(),
        }),
        intent_hash: vec![3; 32],
        pending: true,
        requester: Keypair::ed25519_from_bytes([29; 32])
            .unwrap()
            .public()
            .to_peer_id()
            .to_base58(),
    };
    owner.sign(Domain::MediaConditionalKeep, keep.encode_to_vec())
}

fn wire(record: SignedRecord, version: u64) -> proto::MediaKeepState {
    proto::MediaKeepState {
        owner_present: true,
        version,
        record: Some(record),
    }
}

#[test]
fn a_verified_state_preserves_intent_and_is_bound_to_its_owner() {
    let owner = owner();
    let record = signed(&owner, 1);
    let state = State::verify(&owner, wire(record.clone(), 1)).unwrap();
    assert_eq!(state.revision(), 1);
    assert_eq!(state.hashes(), &[vec![7; 32]]);
    assert_eq!(state.intent_hash(), Some([3; 32]));
    assert!(state.pending());
    assert_eq!(state.record(), Some(&record));
    assert!(state
        .ensure_owner(&Identity::from_secret(&[50; 32]))
        .is_err());
}

#[test]
fn even_absent_state_cannot_be_reused_after_switching_owner() {
    let state = State::verify(&owner(), proto::MediaKeepState::default()).unwrap();
    assert!(state
        .ensure_owner(&Identity::from_secret(&[50; 32]))
        .is_err());
}

#[test]
fn foreign_forged_or_inconsistent_owner_records_are_rejected() {
    let owner = owner();
    assert!(State::verify(
        &owner,
        wire(signed(&Identity::from_secret(&[50; 32]), 1), 1)
    )
    .is_err());
    assert!(State::verify(&owner, wire(signed(&owner, 1), 2)).is_err());
    let mut forged = signed(&owner, 1);
    forged.signature[0] ^= 1;
    assert!(State::verify(&owner, wire(forged, 1)).is_err());
    let absent = proto::MediaKeepState {
        owner_present: false,
        version: 1,
        record: Some(signed(&owner, 1)),
    };
    assert!(State::verify(&owner, absent).is_err());
}

#[test]
fn legacy_records_verify_but_cannot_claim_conditional_metadata() {
    let owner = owner();
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![vec![7; 32]],
        time: 10,
    };
    let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
    let state = State::verify(&owner, wire(record, 1)).unwrap();
    assert!(!state.pending());
    assert_eq!(state.intent_hash(), None);
    let conditional = signed(&owner, 1);
    assert!(conditional.verify(Domain::MediaKeep).is_err());
    let wrong_domain = owner.sign(Domain::MediaKeep, conditional.payload);
    assert!(State::verify(&owner, wire(wrong_domain, 1)).is_err());
}

#[test]
fn malformed_signed_intents_and_unverifiable_unsigned_counters_are_rejected() {
    let owner = owner();
    let record = signed(&owner, 1);
    let mut keep = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    keep.expected.as_mut().unwrap().record_hash = vec![9; 31];
    let malformed = owner.sign(Domain::MediaConditionalKeep, keep.encode_to_vec());
    assert!(State::verify(&owner, wire(malformed, 1)).is_err());
    let unsigned = proto::MediaKeepState {
        owner_present: true,
        version: 4,
        record: None,
    };
    assert!(State::verify(&owner, unsigned).is_err());
    let too_far = proto::MediaKeepState {
        version: u64::MAX,
        ..Default::default()
    };
    assert!(State::verify(&owner, too_far).is_err());
}

#[test]
fn applied_receipt_must_be_the_exact_signed_request() {
    let owner = owner();
    let requested = signed(&owner, 1);
    let mut other = proto::MediaKeepIntent::decode(requested.payload.as_slice()).unwrap();
    other.pending = false;
    let different = owner.sign(Domain::MediaConditionalKeep, other.encode_to_vec());
    let response = proto::MediaResponse {
        status: Status::Ok.into(),
        keep_applied: true,
        keep_state: Some(Box::new(wire(different, 1))),
        ..Default::default()
    };
    assert!(parse_replaced(&owner, &requested, &[vec![7; 32]], response).is_err());
}

#[test]
fn accepted_and_conflict_results_have_distinct_verified_receipts() {
    let owner = owner();
    let record = signed(&owner, 1);
    let state = wire(record.clone(), 1);
    let accepted = proto::MediaResponse {
        status: Status::Ok.into(),
        keep_applied: true,
        keep_state: Some(Box::new(state.clone())),
        missing: vec![vec![7; 32]],
        ..Default::default()
    };
    assert!(matches!(
        parse_replaced(&owner, &record, &[vec![7; 32]], accepted).unwrap(),
        Replaced::Applied { .. }
    ));
    let conflict = proto::MediaResponse {
        status: Status::Stale.into(),
        keep_state: Some(Box::new(state)),
        ..Default::default()
    };
    assert!(matches!(
        parse_replaced(&owner, &record, &[], conflict).unwrap(),
        Replaced::Conflict(_)
    ));
}

#[test]
fn missing_hashes_and_receipt_flags_cannot_authorize_unrequested_uploads() {
    let owner = owner();
    let record = signed(&owner, 1);
    assert!(verify_missing(&[vec![8; 32]], &[vec![7; 32]]).is_err());
    assert!(verify_missing(&[vec![7; 32], vec![7; 32]], &[vec![7; 32], vec![8; 32]]).is_err());
    for (status, applied) in [(Status::Ok, false), (Status::Stale, true)] {
        let response = proto::MediaResponse {
            status: status.into(),
            keep_applied: applied,
            keep_state: Some(Box::new(wire(record.clone(), 1))),
            ..Default::default()
        };
        assert!(parse_replaced(&owner, &record, &[], response).is_err());
    }
}

async fn unsupported_node() -> (Node, Arc<Mutex<Vec<&'static str>>>) {
    let mut server = Box::new(
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap(),
    );
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        }
    })
    .await
    .expect("unsupported peer readiness");
    let requests = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(serve_unsupported(server, Arc::clone(&requests)));
    let node = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .unwrap();
    (node, requests)
}

async fn serve_unsupported(
    mut server: Box<libp2p::Swarm<dyapp_p2p_net::Behaviour>>,
    requests: Arc<Mutex<Vec<&'static str>>>,
) {
    loop {
        if let SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
            message: RequestMessage::Request {
                request, channel, ..
            },
            ..
        })) = server.select_next_some().await
        {
            let kind = match request.request {
                Some(media_request::Request::ConditionalKeep(_)) => "conditional",
                Some(media_request::Request::Keep(_)) => "legacy",
                _ => "other",
            };
            requests.lock().unwrap().push(kind);
            server
                .behaviour_mut()
                .media
                .send_response(
                    channel,
                    proto::MediaResponse {
                        status: Status::Unsupported.into(),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
    }
}

#[tokio::test]
async fn an_unsupported_peer_never_causes_a_weaker_fallback() {
    let (mut node, requests) = unsupported_node().await;
    assert!(matches!(
        node.media_keep_state(&owner()).await,
        Err(Error::Status(Status::Unsupported))
    ));
    assert_eq!(*requests.lock().unwrap(), vec!["conditional"]);
}
