//! Integration tests for dyapp
//! Tests multi-peer scenarios: logical time, video, replication and rate limits

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

    // Close session
    session.close().await.unwrap();
    assert_eq!(session.state().await, SessionState::Closed);
}

#[tokio::test]
async fn test_hybrid_clock_ordering() {
    use dyapp_messaging::hlc::{HybridClock, MessageOrdering};

    let alice_clock = HybridClock::new();
    let bob_clock = HybridClock::new();

    // Alice sends msg1, Bob receives it and replies, Alice receives the reply
    let t1 = alice_clock.now();
    let t2 = {
        bob_clock.observe(t1).unwrap();
        bob_clock.now()
    };
    alice_clock.observe(t2).unwrap();
    let t3 = alice_clock.now();

    // Causality preserved: t1 < t2 < t3
    assert!(t1 < t2 && t2 < t3);

    // Every peer orders the same messages the same way, ties broken by peer ID
    let mut ordering = MessageOrdering::new();
    ordering.add(t3, "alice".into(), "msg3".into());
    ordering.add(t2, "bob".into(), "msg2".into());
    ordering.add(t1, "alice".into(), "msg1".into());
    ordering.add(t1, "aaron".into(), "concurrent".into());
    assert_eq!(
        ordering.get_ordered(),
        vec!["concurrent", "msg1", "msg2", "msg3"]
    );
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
