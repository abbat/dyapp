//! Explicit time and terminal callbacks prove cleanup without guessed waits.
use super::*;

#[tokio::test]
async fn verified_progress_renews_a_transition_but_expired_progress_cannot_revive_it() {
    let mut upload = Upload::new(false);
    upload.queue(upload.peers[0]);
    let before = upload.replicas.puts[&1].deadline;
    upload.now = before.checked_sub(PUT_TIMEOUT / 2).unwrap();
    upload.ancestry(true);
    let extended = upload.now + PUT_TIMEOUT;
    assert_eq!(upload.replicas.puts[&1].deadline, extended);
    let id = *upload.replicas.uploads.requests.keys().next().unwrap();
    let ancestors = upload.chain.records[..3].iter().rev().cloned().collect();
    let reply = upload
        .chain
        .reply(segment(upload.chain.target(), ancestors));
    upload.now = extended;
    upload.replicas.media_response_at(
        &mut upload.swarm,
        &upload.chain.source,
        upload.peers[0],
        id,
        reply,
        upload.now,
    );
    assert!(upload.replicas.puts.is_empty());
    assert!(upload.replicas.uploads.states.is_empty());
    assert!(upload.replicas.uploads.requests.is_empty());
    assert!(upload.replicas.media_queue.is_empty());
    assert!(upload.replicas.media_outgoing.is_empty());
    assert!(upload.replicas.media_puts.is_empty());
}

#[tokio::test]
async fn absent_or_unproven_receipts_never_extend_the_upload_deadline_or_confirm_a_write() {
    for progress in [
        None,
        Some(proto::MediaKeepProgress {
            ready: true,
            next_version: 0,
            target_current: None,
        }),
    ] {
        let mut upload = Upload::new(false);
        upload.queue(upload.peers[0]);
        let before = upload.replicas.puts[&1].deadline;
        upload.now = before.checked_sub(PUT_TIMEOUT / 2).unwrap();
        let id = *upload.replicas.uploads.requests.keys().next().unwrap();
        let reply = proto::MediaResponse {
            status: proto::Status::Ok.into(),
            keep_progress: progress,
            ..Default::default()
        };
        upload.replicas.media_response_at(
            &mut upload.swarm,
            &upload.chain.source,
            upload.peers[0],
            id,
            reply,
            upload.now,
        );
        let put = &upload.replicas.puts[&1];
        assert_eq!(put.deadline, before);
        assert!(put.stored.is_empty());
        assert!(put.shard_stored.is_empty());
        assert!(upload.replicas.media_puts.is_empty());
        assert!(upload.replicas.uploads.requests.is_empty());
        assert!(upload.replicas.media_queue.is_empty());
    }
}

#[tokio::test]
async fn pruning_an_expired_parent_keeps_only_bounded_flight_accounting_until_the_reply() {
    let mut upload = Upload::new(true);
    upload.queue(upload.peers[0]);
    let id = *upload.replicas.uploads.requests.keys().next().unwrap();
    let before = upload.replicas.puts[&1].deadline;
    upload
        .replicas
        .finish_puts(&mut upload.swarm, &upload.chain.source, before);
    assert!(upload.replicas.puts.is_empty());
    assert!(upload.replicas.uploads.states.is_empty());
    assert_eq!(upload.replicas.uploads.requests.len(), 1);
    assert_eq!(upload.replicas.media_outgoing.len(), 1);
    let reply = upload.chain.reply(segment(upload.chain.target(), vec![]));
    upload.replicas.media_response_at(
        &mut upload.swarm,
        &upload.chain.source,
        upload.peers[0],
        id,
        reply,
        before,
    );
    assert!(upload.replicas.uploads.requests.is_empty());
    assert!(upload.replicas.media_outgoing.is_empty());
    assert!(upload.replicas.media_queue.is_empty());
    assert!(upload.replicas.puts.is_empty());
}

#[tokio::test]
async fn conflicting_current_ownership_is_preserved_without_writes_or_confirmation() {
    let mut upload = Upload::new(false);
    let ancestors = upload.chain.records[..3].iter().rev().cloned().collect();
    let response = upload
        .chain
        .reply(segment(upload.chain.target(), ancestors));
    assert_eq!(response.status, i32::from(proto::Status::Ok));
    let owner = Identity::from_secret(&[62; 32]);
    let newer = signed(
        &upload.chain.destination,
        &owner,
        dyapp_identity::sha256(b"newer").to_vec(),
    );
    upload.queue(upload.peers[0]);
    upload.ancestry(true);
    assert!(upload.replicas.puts[&1].stored.is_empty());
    assert!(upload.replicas.media_puts.is_empty());
    assert!(upload.replicas.media_outgoing.is_empty());
    assert!(upload.replicas.media_queue.is_empty());
    let snapshot = upload
        .chain
        .destination
        .media
        .as_ref()
        .unwrap()
        .keep_snapshot(&owner.peer_id())
        .unwrap();
    assert_eq!(snapshot.record.as_ref(), Some(&newer));
}

#[tokio::test]
async fn busy_peer_and_global_stream_limits_preserve_queued_parts_without_starting_admission() {
    for count in [2, 16] {
        let mut upload = Upload::new(true);
        for _ in 0..count {
            let to = if count == 2 {
                upload.peers[0]
            } else {
                PeerId::random()
            };
            let id = upload
                .swarm
                .behaviour_mut()
                .media
                .send_request(&to, proto::MediaRequest::default());
            upload.replicas.media_outgoing.insert(id, to);
        }
        upload.queue(upload.peers[0]);
        let expected = 1 + upload.replicas.puts[&1].shard_sent.len();
        assert_eq!(upload.replicas.media_queue.len(), expected);
        assert_eq!(upload.replicas.media_outgoing.len(), count);
        assert!(upload.replicas.uploads.requests.is_empty());
        assert!(upload.replicas.media_puts.is_empty());
        assert!(upload.replicas.shard_puts.is_empty());
        assert!(upload.replicas.trust.trusted(&upload.peers[0]));
    }
}

#[tokio::test]
async fn denied_owner_sends_no_payload_and_does_not_penalize_the_holder() {
    let mut upload = Upload::new(true);
    let owner = Identity::from_secret(&[62; 32]);
    upload
        .chain
        .source
        .deny
        .write()
        .unwrap()
        .insert(owner.peer_id());
    upload.queue(upload.peers[0]);
    assert!(upload.replicas.media_outgoing.is_empty());
    assert!(upload.replicas.uploads.requests.is_empty());
    assert!(upload.replicas.media_puts.is_empty());
    assert!(upload.replicas.shard_puts.is_empty());
    assert!(upload.replicas.puts[&1].stored.is_empty());
    assert!(upload.replicas.trust.trusted(&upload.peers[0]));
}
