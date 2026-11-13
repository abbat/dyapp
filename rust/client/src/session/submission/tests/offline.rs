use super::*;
use crate::forwarding::tests::open_chat;

#[tokio::test]
async fn established_text_queues_without_profiles_and_reopens_exact_writes() {
    let mut f = fixture();
    open_chat(&mut f.session.store, &f.peer);
    let result = f
        .session
        .try_submit_established_text(&mut [], &f.peer, b"offline local", &f.seal)
        .await
        .unwrap()
        .unwrap();
    let id = result.id();
    assert!(matches!(result, Submission::Queued { .. }));
    let before = writes(&f.session);
    assert_ne!(before, [] as [Vec<u8>; 0]);
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    assert_eq!(writes(&reopened), before);
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), before);
    let rows = reopened.store().current_chat_messages(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    assert_eq!(
        rows[0].current.text.as_deref(),
        Some(b"offline local".as_slice())
    );
}

#[tokio::test]
async fn missing_pair_is_distinct_from_refused_existing_pair_without_staging() {
    let mut f = fixture();
    let missing = f
        .session
        .try_submit_established_text(&mut [], &"ef".repeat(32), b"text", &f.seal)
        .await
        .unwrap();
    assert!(missing.is_none());
    assert!(matches!(
        f.session
            .try_submit_established_text(&mut [], &f.peer, b"text", &f.seal)
            .await,
        Err(Error::ConversationNotOpen)
    ));
    open_chat(&mut f.session.store, &f.peer);
    f.session.store.set_blocked(&f.peer, true).unwrap();
    assert!(matches!(
        f.session
            .try_submit_established_text(&mut [], &f.peer, b"text", &f.seal)
            .await,
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
    assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
}
