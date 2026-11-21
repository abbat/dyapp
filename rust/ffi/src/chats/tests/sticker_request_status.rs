//! Native status projection retains authoritative core phase, generation and retry state.
use super::offline::snapshot;
use super::saved_sticker::save;
use super::sticker_exchange::Received;
use crate::{
    StickerPackSaveControl as Control, StickerPackSaveFailure as Failure,
    StickerPackSaveState as State, StickerPackSaveStatus,
};
use std::path::Path;

fn read(f: &Received) -> StickerPackSaveStatus {
    f.native
        .chats
        .chat_sticker_pack_status(
            f.peer.clone(),
            f.key.group.clone(),
            f.key.author.clone(),
            f.key.id.to_vec(),
        )
        .unwrap()
}

fn assert_new_and_waiting(f: &Received) -> Vec<u8> {
    let new = read(f);
    assert_eq!(new.state, State::New);
    assert_eq!(new.controls, vec![Control::Request]);
    f.request();
    let waiting = read(f);
    assert_eq!(waiting.state, State::Waiting);
    assert_eq!(waiting.controls, vec![Control::StopWaiting]);
    let token = waiting.generation.unwrap();
    assert_eq!(token.len(), 16);
    assert!(waiting.retry_at_ms.is_none() && waiting.failure.is_none());
    token
}

fn assert_answer_and_failure(f: &Received, token: Vec<u8>) {
    assert!(f.answer());
    let answer = read(f);
    assert_eq!(answer.state, State::Downloading);
    assert_eq!(answer.controls, vec![Control::StopWaiting]);
    assert_eq!(answer.generation.as_ref(), Some(&token));
    assert_eq!(answer.retry_at_ms, Some(0));
    assert_eq!(f.run(), 0);
    let failed = read(f);
    assert_eq!(failed.state, State::Failed);
    assert_eq!(failed.failure, Some(Failure::Fetch));
    assert_eq!(failed.controls, vec![Control::Retry, Control::StopWaiting]);
    assert_eq!(failed.generation, Some(token));
    let session = f.native.chats.session.lock().unwrap();
    let stored = session
        .store()
        .sticker_pack_download_state(&f.peer, f.pack.id())
        .unwrap()
        .unwrap();
    assert_eq!(failed.retry_at_ms, Some(stored.retry_at_ms));
}

#[test]
fn native_status_preserves_new_waiting_answer_failure_generation_and_deadline() {
    let f = Received::new();
    let token = assert_new_and_waiting(&f);
    assert_answer_and_failure(&f, token);
}

#[test]
fn native_saved_status_reconciles_intent_without_new_mail_or_key_state() {
    let f = Received::new();
    f.request();
    save(&f.native, &f.pack, &f.native.chats.seal_key);
    let before = snapshot(&f.native);
    assert_eq!(
        read(&f),
        StickerPackSaveStatus {
            state: State::Saved,
            generation: None,
            failure: None,
            retry_at_ms: None,
            controls: vec![crate::StickerPackSaveControl::Manage],
        }
    );
    assert_eq!(snapshot(&f.native), before);
    assert!(f
        .native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_none());
}

#[test]
fn native_unreadable_status_preserves_generation_and_source_file() {
    let f = Received::new();
    f.request();
    let token = read(&f).generation;
    save(&f.native, &f.pack, &f.native.chats.seal_key);
    let file = Path::new(&f.native.dir)
        .join("sticker-packs")
        .join(dyapp_identity::hex(f.pack.id()));
    std::fs::write(&file, b"unreadable native status fixture").unwrap();
    let current = read(&f);
    assert_eq!(current.state, State::Failed);
    assert_eq!(current.failure, Some(Failure::Storage));
    assert_eq!(current.generation, token);
    assert_eq!(
        std::fs::read(file).unwrap(),
        b"unreadable native status fixture"
    );
}

#[test]
fn invalid_or_blocked_status_is_an_error_not_an_empty_or_saved_result() {
    let f = Received::new();
    for (group, author, id) in [
        (vec![], f.key.author.clone(), f.key.id.to_vec()),
        (f.key.group.clone(), "other".into(), f.key.id.to_vec()),
        (f.key.group.clone(), f.key.author.clone(), vec![]),
    ] {
        assert_eq!(
            f.native
                .chats
                .chat_sticker_pack_status(f.peer.clone(), group, author, id,)
                .unwrap_err()
                .to_string(),
            "unknownMessage"
        );
    }
    f.native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&f.peer, true)
        .unwrap();
    assert_eq!(
        f.native
            .chats
            .chat_sticker_pack_status(f.peer, f.key.group, f.key.author, f.key.id.to_vec(),)
            .unwrap_err()
            .to_string(),
        "blocked"
    );
}
