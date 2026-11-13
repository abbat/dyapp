//! An established offline operation retains its identity through restart and real-node retry.
use super::*;

async fn matched_pair(
    alice: &mut Person,
    bob: &mut Person,
    path: &std::path::Path,
) -> (Session, Session, Vec<dyapp_profile::VerifiedProfile>) {
    let owners = [alice.identity.peer_id(), bob.identity.peer_id()];
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &owners[1], 1)
        .await
        .unwrap();
    let mut held = Vec::new();
    for owner in &owners {
        held.push(alice.node.get_profile(owner).await.unwrap());
    }
    let (mut ours, mut theirs, group) = begin_chat_at(alice, bob, &held, path).await;
    for (session, person) in [(&mut ours, &mut *alice), (&mut theirs, &mut *bob)] {
        session
            .interact(from_mut(&mut person.node), &group, Kind::Like, &SEAL)
            .await
            .unwrap();
    }
    for (session, person, peer) in [
        (&mut ours, alice, &owners[1]),
        (&mut theirs, bob, &owners[0]),
    ] {
        session
            .receive(from_mut(&mut person.node), &person.device_key, &held, &SEAL)
            .await
            .unwrap();
        assert_eq!(session.conversation(peer).unwrap(), Conversation::Open);
    }
    (ours, theirs, held)
}

fn pending_bytes(session: &Session) -> Vec<Vec<u8>> {
    session
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .map(|write| write.as_bytes().to_vec())
        .collect()
}

#[tokio::test]
async fn offline_text_reopens_and_retries_through_a_real_node_without_duplicate_delivery() {
    let address = link_node().await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let path = store_path("offline-retry");
    let (mut ours, mut theirs, held) = matched_pair(&mut alice, &mut bob, &path).await;
    let queued = ours
        .try_submit_established_text(&mut [], &bob.identity.peer_id(), b"retained offline", &SEAL)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        queued,
        dyapp_client::session::Submission::Queued { .. }
    ));
    let id = queued.id();
    let bytes = pending_bytes(&ours);
    assert_ne!(bytes, [] as [Vec<u8>; 0]);
    drop(ours);
    let mut reopened = Session::open(Store::open(&path, &alice.identity).unwrap(), &SEAL).unwrap();
    assert_eq!(pending_bytes(&reopened), bytes);
    let now_ms = super::super::now_ms();
    let due = reopened.due_outbound(now_ms).unwrap();
    assert!(!due.is_empty());
    reopened
        .retry_due(from_mut(&mut alice.node), now_ms, &SEAL)
        .await
        .unwrap();
    check_receipt(&mut theirs, &mut bob, &held, &alice.identity.peer_id(), id).await;
    // Replay after receipt so the second fetch also exercises recipient deduplication.
    for write in &due {
        alice.node.put_prepared(write).await.unwrap();
    }
    check_receipt(&mut theirs, &mut bob, &held, &alice.identity.peer_id(), id).await;
}

async fn check_receipt(
    session: &mut Session,
    person: &mut Person,
    held: &[dyapp_profile::VerifiedProfile],
    peer: &str,
    id: [u8; 16],
) {
    session
        .receive(from_mut(&mut person.node), &person.device_key, held, &SEAL)
        .await
        .unwrap();
    let rows = session.store().current_chat_messages(peer).unwrap();
    let matching: Vec<_> = rows.iter().filter(|row| row.key.id == id).collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(
        matching[0].current.text.as_deref(),
        Some(b"retained offline".as_slice())
    );
}
