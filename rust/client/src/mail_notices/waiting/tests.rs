use super::*;
use crate::mail_notices::tests::row;
use crate::store::Conversation;

fn snapshot(owner: &str, rows: Vec<ChatRow>, likes: usize) -> Waiting {
    Waiting::new(owner.into(), rows, likes).unwrap()
}

#[test]
fn disabled_unchanged_decreasing_or_read_request_counts_do_not_post() {
    let empty = snapshot("owner", vec![], 0);
    let unread = snapshot(
        "owner",
        vec![row("peer", Conversation::Open, "private", 1)],
        0,
    );
    assert_eq!(
        decide(&empty, &unread, false, NoticeContent::Text).unwrap(),
        None
    );
    assert_eq!(
        decide(&unread, &unread, true, NoticeContent::Text).unwrap(),
        None
    );
    assert_eq!(
        decide(&unread, &empty, true, NoticeContent::Text).unwrap(),
        None
    );
    let read_request = snapshot(
        "owner",
        vec![row(
            "peer",
            Conversation::RequestReceived([1; 16]),
            "read",
            0,
        )],
        0,
    );
    assert_eq!(
        decide(&empty, &read_request, true, NoticeContent::Text).unwrap(),
        None
    );
}

#[test]
fn like_and_request_arrivals_are_fixed_but_open_text_can_add_sender() {
    let before = snapshot("owner", vec![], 0);
    let likes = snapshot("owner", vec![], 1);
    let fixed = Some(Notice {
        peer_id: None,
        preview: Preview::Empty,
    });
    assert_eq!(
        decide(&before, &likes, true, NoticeContent::Text).unwrap(),
        fixed
    );
    let request = snapshot(
        "owner",
        vec![row(
            "stranger",
            Conversation::RequestReceived([1; 16]),
            "secret",
            1,
        )],
        0,
    );
    assert_eq!(
        decide(&before, &request, true, NoticeContent::Text).unwrap(),
        fixed
    );
    let open = snapshot(
        "owner",
        vec![row("peer", Conversation::Open, "hello", 1)],
        0,
    );
    assert_eq!(
        decide(&before, &open, true, NoticeContent::Fixed).unwrap(),
        fixed
    );
    let named = decide(&before, &open, true, NoticeContent::Name)
        .unwrap()
        .unwrap();
    assert_eq!(named.peer_id.as_deref(), Some("peer"));
    assert_eq!(named.preview, Preview::Empty);
    assert_eq!(
        decide(&before, &open, true, NoticeContent::Text)
            .unwrap()
            .unwrap()
            .preview,
        Preview::Text("hello".into())
    );
}

#[test]
fn owner_mismatch_and_count_overflow_fail_without_exposing_content() {
    let before = snapshot("one", vec![], 0);
    let after = snapshot(
        "two",
        vec![row("peer", Conversation::Open, "private", 1)],
        0,
    );
    assert!(matches!(
        decide(&before, &after, true, NoticeContent::Text),
        Err(Error::IdentityChanged)
    ));
    assert!(matches!(
        Waiting::new(
            "owner".into(),
            vec![row("peer", Conversation::Open, "private", usize::MAX)],
            1
        ),
        Err(Error::CountOverflow)
    ));
}

fn sizes(store: &Store) -> (usize, usize) {
    let waiting = capture(store).unwrap();
    (waiting.rows.len(), waiting.count)
}

fn like(store: &mut Store, sender: &str, target: &str, n: u8) {
    store
        .apply_interaction(
            sender,
            target,
            &[n; 32],
            crate::interactions::Interaction {
                kind: crate::interactions::Kind::Like,
                event_id: [n; 16],
                hlc: u64::from(n) << 16,
            },
        )
        .unwrap();
}

#[test]
fn store_capture_excludes_muted_blocked_likes_and_rows_without_changing_private_state() {
    let (owner, peer) = (
        dyapp_identity::Identity::generate(),
        dyapp_identity::Identity::generate(),
    );
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/core-notice-waiting")
        .join(owner.peer_id());
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = Store::open(&dir.join("client.db"), &owner).unwrap();
    let (me, them) = (owner.peer_id(), peer.peer_id());
    like(&mut store, &them, &me, 1);
    assert_eq!(sizes(&store), (0, 1));
    store.set_muted(&them, true).unwrap();
    assert_eq!(sizes(&store), (0, 0));
    like(&mut store, &me, &them, 2);
    assert_eq!(sizes(&store), (0, 0));
    store.set_muted(&them, false).unwrap();
    assert_eq!(sizes(&store), (1, 0));
    store.set_blocked(&them, true).unwrap();
    assert_eq!(sizes(&store), (0, 0));
    store.set_blocked(&them, false).unwrap();
    assert_eq!(sizes(&store), (1, 0));
    assert!(store.likes(&them).unwrap().ours);
    assert!(store.likes(&them).unwrap().matched());
}

#[test]
fn real_store_snapshots_cannot_mix_two_identity_owners() {
    let first = crate::sticker_media::tests::Fixture::new(true, false);
    let second = crate::sticker_media::tests::Fixture::new(true, false);
    let before = capture(&first.store).unwrap();
    let after = capture(&second.store).unwrap();
    assert!(matches!(
        decide(&before, &after, true, NoticeContent::Text),
        Err(Error::IdentityChanged)
    ));
}
