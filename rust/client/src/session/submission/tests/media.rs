mod upload;

use super::*;
use crate::chat::sticker::Sticker;
use crate::interactions::Kind;
use crate::store::Conversation;

fn open_chat(mut f: Fixture) -> Fixture {
    let owner = f.owner.peer_id();
    let store = &mut f.session.store;
    let like = store
        .interact(&f.peer, Kind::Like, &[1], 1_000)
        .unwrap()
        .unwrap();
    store
        .apply_interaction(&f.peer, &owner, &[2], like)
        .unwrap();
    assert_eq!(store.conversation(&f.peer).unwrap(), Conversation::Open);
    f
}

fn ineligible_chat(blocked: bool) -> Fixture {
    let mut f = open_chat(fixture());
    if blocked {
        f.session.store().set_blocked(&f.peer, true).unwrap();
    } else {
        f.session
            .store
            .interact(&f.peer, Kind::Unlike, &[1], 2_000)
            .unwrap();
        assert_eq!(
            f.session.store().conversation(&f.peer).unwrap(),
            Conversation::Closed
        );
    }
    f
}

fn sticker() -> Sticker {
    Sticker {
        id: [99; 16],
        hlc: 0,
        pack_id: [2; 32],
        sha256: [3; 32],
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: None,
    }
}

fn queued(result: &Submission) -> [u8; 16] {
    let id = result.id();
    assert!(matches!(
        result,
        Submission::Queued {
            error: Error::Client(crate::Error::Status(Status::NotFound)),
            ..
        }
    ));
    id
}

#[tokio::test]
async fn forwarded_submission_keeps_identity_and_exact_bytes_after_reopen_and_retry() {
    let mut f = open_chat(fixture());
    let id = queued(
        &f.session
            .submit_forwarded_text_to(&mut [], &f.peer, b"forwarded", &f.seal)
            .await
            .unwrap(),
    );
    let rows = f.session.store().current_chat_messages(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    assert!(rows[0].forwarded);
    assert_eq!(
        rows[0].current.text.as_deref(),
        Some(b"forwarded".as_slice())
    );
    let before = writes(&f.session);
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), before);
    let rows = reopened.store().current_chat_messages(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    assert!(rows[0].forwarded);
}

#[tokio::test]
async fn sticker_submission_keeps_fallback_and_fresh_id_after_reopen_and_retry() {
    let mut f = open_chat(fixture());
    let template = sticker();
    let id = queued(
        &f.session
            .submit_sticker_to(&mut [], &f.peer, &template, &f.seal)
            .await
            .unwrap(),
    );
    assert_ne!(id, template.id);
    let rows = f.session.store().current_chat_stickers(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    assert_eq!(rows[0].sticker.emoji, template.emoji);
    assert_eq!(rows[0].sticker.pack_id, template.pack_id);
    let before = writes(&f.session);
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), before);
    let rows = reopened.store().current_chat_stickers(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
}

#[tokio::test]
async fn closed_and_blocked_preflight_never_reports_a_committed_submission() {
    for blocked in [false, true] {
        let mut f = ineligible_chat(blocked);
        let sealed = f.session.store().sealed_device().unwrap();
        let before = writes(&f.session);
        let forward = f
            .session
            .submit_forwarded_text_to(&mut [], &f.peer, b"no", &f.seal)
            .await
            .unwrap_err();
        let sticker = f
            .session
            .submit_sticker_to(&mut [], &f.peer, &sticker(), &f.seal)
            .await
            .unwrap_err();
        for error in [forward, sticker] {
            if blocked {
                assert!(matches!(
                    error,
                    Error::Store(crate::store::Error::BlockedPeer)
                ));
            } else {
                assert!(matches!(error, Error::ConversationNotOpen));
            }
        }
        assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
        assert_eq!(writes(&f.session), before);
    }
}

#[tokio::test]
async fn invalid_sticker_is_refused_before_mls_or_outbox_commit() {
    let mut f = open_chat(fixture());
    let mut invalid = sticker();
    invalid.emoji = "two labels".into();
    let sealed = f.session.store().sealed_device().unwrap();
    let before = writes(&f.session);
    assert!(matches!(
        f.session
            .submit_sticker_to(&mut [], &f.peer, &invalid, &f.seal)
            .await,
        Err(Error::Client(crate::Error::Reply("bad sticker")))
    ));
    assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
    assert_eq!(writes(&f.session), before);
}

#[tokio::test]
async fn strict_forward_and_sticker_sends_keep_the_node_acceptance_contract() {
    let mut f = open_chat(fixture());
    assert!(matches!(
        f.session
            .send_forwarded_text_to(&mut [], &f.peer, b"forwarded", &f.seal)
            .await,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    assert!(matches!(
        f.session
            .send_sticker_to(&mut [], &f.peer, &sticker(), &f.seal)
            .await,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    assert_eq!(
        f.session
            .store()
            .current_chat_messages(&f.peer)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        f.session
            .store()
            .current_chat_stickers(&f.peer)
            .unwrap()
            .len(),
        1
    );
}
