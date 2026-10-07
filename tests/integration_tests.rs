//! Integration tests for dyapp
//! Tests multi-peer scenarios: messaging, video, profile sync

use dyapp_messaging::message::{Message, MessageStatus};
use dyapp_messaging::queue::MessageQueue;

#[tokio::test]
async fn test_message_offline_queue_and_delivery() {
    let mut queue = MessageQueue::new();

    // Create message when peer offline
    let msg = Message::new(
        "msg1".to_string(),
        "alice".to_string(),
        "bob".to_string(),
        "Hello Bob!".to_string(),
        1,
    );

    queue.enqueue(msg.clone()).unwrap();
    assert_eq!(queue.size(), 1);
    assert!(queue.peek().is_some());

    // Message still pending
    let pending = queue.get_pending();
    assert_eq!(pending.len(), 1);

    // Mark delivered when peer comes online
    queue.mark_delivered("msg1").unwrap();
    let entry = queue.peek().unwrap();
    assert_eq!(entry.message.status, MessageStatus::Delivered);

    // Clean up
    queue.remove_delivered();
    assert_eq!(queue.size(), 0);
}

#[tokio::test]
async fn test_multi_peer_message_retry() {
    let mut queue = MessageQueue::new();

    // Alice sends to Bob (offline)
    let msg = Message::new(
        "msg1".to_string(),
        "alice".to_string(),
        "bob".to_string(),
        "Try 1".to_string(),
        1,
    );

    queue.enqueue(msg).unwrap();

    // Simulate delivery failure
    if let Some(mut entry) = queue.dequeue() {
        entry.message.status = MessageStatus::Failed;
        queue.enqueue(entry.message).unwrap();
    }

    // Wait for retry
    // Failed messages are immediately eligible for the first retry.

    let retryable = queue.retry_failed();
    assert_eq!(retryable.len(), 1);
    assert_eq!(retryable[0].retry_count, 1);
    assert_eq!(retryable[0].message.status, MessageStatus::Pending);
}

#[tokio::test]
async fn test_signed_profile_over_http() {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use dyapp_bootstrap::{
        api::{AppState, ProfileList},
        rate_limit::PeerRateLimiter,
        BootstrapConfig, BootstrapServer, BootstrapStore,
    };
    use dyapp_identity::{Identity, SignedRecord};
    use dyapp_profile::Profile;
    use prost::Message;
    use std::sync::Arc;
    use tower::ServiceExt;

    let router = BootstrapServer::router(AppState {
        store: Arc::new(
            BootstrapStore::new(&format!("/tmp/ai/integration-api-{}", uuid::Uuid::new_v4()))
                .unwrap(),
        ),
        rate_limiter: Arc::new(PeerRateLimiter::new(100)),
        config: BootstrapConfig::default(),
    });
    let call = |method: &str, uri: String, body: Vec<u8>| {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::from(body))
            .unwrap();
        let router = router.clone();
        async move {
            let response = router.oneshot(request).await.unwrap();
            let status = response.status();
            (
                status,
                to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            )
        }
    };

    let alice = Identity::generate();
    let profile = |version| Profile {
        version,
        age: 25,
        country: "US".into(),
        ..Profile::default()
    };
    let v2 = profile(2).sign(&alice).encode_to_vec();
    let post = |body: Vec<u8>| call("POST", "/profiles".into(), body);

    assert_eq!(post(v2.clone()).await.0, StatusCode::CREATED);
    assert_eq!(
        post(profile(1).sign(&alice).encode_to_vec()).await.0,
        StatusCode::CONFLICT
    );
    let mut forged = profile(3).sign(&alice);
    forged.public_key = Identity::generate().public_key().to_vec();
    assert_eq!(
        post(forged.encode_to_vec()).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(post(vec![0xff; 3]).await.0, StatusCode::BAD_REQUEST);

    let (status, body) = call("GET", format!("/profiles/{}", alice.peer_id()), vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), v2.as_slice());
    let (_, body) = call("GET", "/profiles".into(), vec![]).await;
    assert_eq!(ProfileList::decode(body).unwrap().profiles.len(), 1);

    // Deletion is a signed tombstone: hidden from the list, still served by peer id.
    let tombstone = Profile::tombstone(3).sign(&alice).encode_to_vec();
    assert_eq!(post(tombstone.clone()).await.0, StatusCode::CREATED);
    let (_, body) = call("GET", "/profiles".into(), vec![]).await;
    assert!(ProfileList::decode(body).unwrap().profiles.is_empty());
    let (_, body) = call("GET", format!("/profiles/{}", alice.peer_id()), vec![]).await;
    assert_eq!(
        SignedRecord::decode(body).unwrap().encode_to_vec(),
        tombstone
    );
    assert_eq!(
        call("GET", "/profiles/nobody".into(), vec![]).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_video_session_lifecycle() {
    use dyapp_video::{SessionState, VideoSession};

    let session = VideoSession::new().await.expect("Failed to create session");
    assert_eq!(session.state().await, SessionState::Idle);

    let session_id = session.session_id();
    assert!(!session_id.is_empty());

    // Offer creation
    let _offer = session.create_offer().await.unwrap();
    assert_eq!(session.state().await, SessionState::OfferCreated);

    // Mark offer sent
    session.mark_offer_sent().await.unwrap();
    assert_eq!(session.state().await, SessionState::OfferSent);

    // Connection established
    session.mark_connected().await.unwrap();
    assert_eq!(session.state().await, SessionState::Connected);

    // Close session
    session.close().await.unwrap();
    assert_eq!(session.state().await, SessionState::Closed);
}

#[tokio::test]
async fn test_bootstrap_message_relay() {
    use dyapp_bootstrap::{BootstrapStore, MessageBlob};

    let store = BootstrapStore::new(&format!(
        "/tmp/ai/integration-bootstrap-{}",
        uuid::Uuid::new_v4()
    ))
    .expect("Failed to create store");

    let msg = MessageBlob {
        id: "aaa-message".to_string(),
        sender_id: "alice".to_string(),
        recipient_id: "bob".to_string(),
        encrypted_payload: vec![1, 2, 3, 4],
        timestamp: 1000,
        ttl_expires_at: 2000,
    };

    store.store_message(msg.clone()).unwrap();
    let mut other = msg.clone();
    other.id = "zzz-message".to_string();
    other.recipient_id = "carol".to_string();
    store.store_message(other).unwrap();

    // Bob queries bootstrap for his messages
    let messages = store.get_messages_for_peer("bob").unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "aaa-message");

    // Delete after delivery
    store.delete_message("aaa-message").unwrap();
    let deleted = store.get_message("aaa-message").unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
async fn test_lamport_clock_ordering() {
    use dyapp_messaging::lamport_clock::LamportClock;

    let alice_clock = LamportClock::new();
    let bob_clock = LamportClock::new();

    // Alice sends msg1
    let t1 = alice_clock.increment();
    assert_eq!(t1, 1);

    // Bob receives, updates his clock
    bob_clock.observe(t1);
    let t2 = bob_clock.increment();
    assert_eq!(t2, 3);

    // Alice receives msg from Bob, updates her clock
    alice_clock.observe(t2);
    let t3 = alice_clock.increment();
    assert_eq!(t3, 5);

    // Causality preserved: t1 < t2 < t3
    assert!(t1 < t2 && t2 < t3);
}

#[tokio::test]
async fn test_ice_candidate_priority() {
    use dyapp_video::ice::ICECandidate;

    let host = ICECandidate::new("candidate:1 typ host".to_string(), None, None);
    let srflx = ICECandidate::new("candidate:2 typ srflx".to_string(), None, None);
    let relay = ICECandidate::new("candidate:3 typ relay".to_string(), None, None);

    assert!(host.priority() > srflx.priority());
    assert!(srflx.priority() > relay.priority());
}

#[tokio::test]
async fn test_codec_negotiation() {
    use dyapp_video::codec::{CodecFormat, CodecInfo, CodecNegotiation};

    let local = vec![CodecInfo::vp8(), CodecInfo::h264()];
    let mut negotiation = CodecNegotiation::new(local);

    let remote = vec![CodecInfo::h264(), CodecInfo::av1()];
    negotiation.set_remote_codecs(remote);

    assert!(negotiation.has_agreed());
    assert_eq!(
        negotiation.agreed_codec().unwrap().format,
        CodecFormat::H264
    );
}

#[tokio::test]
async fn test_replication_fault_tolerance() {
    use dyapp_bootstrap::replication::Replication;

    let rep = Replication::new(2, 1).expect("Failed to create replication");

    let data = b"Important message for Bob";
    let fragments = rep.encode(data).expect("Failed to encode");
    assert_eq!(fragments.len(), 3); // 2 data + 1 parity

    // Can decode with 2 of 3 shards
    let decoded = rep
        .decode(&fragments[..2], data.len())
        .expect("Failed to decode");
    assert_eq!(&decoded, data);

    // Can also decode with different 2 of 3
    let decoded2 = rep
        .decode(&[fragments[0].clone(), fragments[2].clone()], data.len())
        .expect("Failed to decode");
    assert_eq!(&decoded2, data);
}

#[tokio::test]
async fn test_rate_limiting() {
    use dyapp_bootstrap::rate_limit::PeerRateLimiter;

    let limiter = PeerRateLimiter::new(100);

    // Peer within limit
    assert!(limiter.check_limit("alice"));

    // Different peer has fresh limit
    assert!(limiter.check_limit("bob"));

    assert_eq!(limiter.get_active_peers(), 2);
}
