use super::offline::snapshot;
use super::sticker_exchange::Received;
use super::*;
use crate::{
    StickerPackSaveAction as Action, StickerPackSaveAttempt as Attempt,
    StickerPackSaveState as State,
};

fn change(f: &Received, action: Action) -> Result<Attempt, CoreError> {
    f.native.chats.change_sticker_pack_request(
        f.peer.clone(),
        f.key.group.clone(),
        f.key.author.clone(),
        f.key.id.to_vec(),
        action,
    )
}
fn ready(f: &Received, action: Action) -> StickerPackSaveStatus {
    let Attempt::Ready { status } = change(f, action).unwrap() else {
        panic!("the local status must be available");
    };
    status
}
fn token(f: &Received, action: Action) -> Vec<u8> {
    let status = ready(f, action);
    assert_eq!(status.state, State::Waiting);
    status.generation.unwrap()
}
fn unpublish(f: &Received) {
    f.native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .save_profile_draft(&dyapp_profile::Profile::default())
        .unwrap();
}

#[test]
fn native_actions_queue_offline_preserve_duplicates_and_fresh_generation_across_restart() {
    let mut f = Received::new();
    let first = token(&f, Action::Request);
    let before = snapshot(&f.native);
    assert_eq!(token(&f, Action::Request), first);
    assert_eq!(snapshot(&f.native), before);
    let next = token(
        &f,
        Action::Fresh {
            generation: first.clone(),
        },
    );
    assert_ne!(next, first);
    let writes = snapshot(&f.native);
    assert!(writes.1.len() > before.1.len());
    drop(f.native.chats);
    f.native.chats = ChatSession::open(
        f.native.dir.clone(),
        f.native.key.clone(),
        f.native.device.clone(),
        Arc::new(TestStore),
        f.native.wrapped.clone(),
    )
    .unwrap();
    assert_eq!(token(&f, Action::Request), next);
    assert_eq!(snapshot(&f.native), writes);
    assert_eq!(
        change(&f, Action::StopWaiting { generation: first })
            .unwrap_err()
            .to_string(),
        "requestChanged"
    );
    let stopped = ready(&f, Action::StopWaiting { generation: next });
    assert_eq!(stopped.state, State::New);
    assert!(stopped.generation.is_none());
    assert_eq!(snapshot(&f.native), writes);
}

#[test]
fn native_retry_rotates_generation_without_mls_or_publication_and_preserves_answer() {
    let f = Received::new();
    let first = token(&f, Action::Request);
    assert!(f.answer());
    assert_eq!(f.run(), 0);
    unpublish(&f);
    let before = snapshot(&f.native);
    let retried = ready(
        &f,
        Action::Retry {
            generation: first.clone(),
        },
    );
    assert_eq!(retried.state, State::Downloading);
    assert!(retried.failure.is_none());
    assert_eq!(retried.retry_at_ms, Some(0));
    let next = retried.generation.unwrap();
    assert_ne!(next, first);
    assert_eq!(
        change(&f, Action::StopWaiting { generation: first })
            .unwrap_err()
            .to_string(),
        "requestChanged"
    );
    assert_eq!(
        ready(&f, Action::StopWaiting { generation: next }).state,
        State::New
    );
    assert_eq!(snapshot(&f.native), before);
}

#[test]
fn native_actions_refuse_unlisted_new_controls_without_changing_session_or_intent() {
    let f = Received::new();
    let first = token(&f, Action::Request);
    {
        let session = f.native.chats.session.lock().unwrap();
        let mut own = session.store().profile_draft().unwrap().unwrap();
        own.devices.clear();
        session.store().save_profile_draft(&own).unwrap();
    }
    let before = snapshot(&f.native);
    assert_eq!(
        change(
            &f,
            Action::Fresh {
                generation: first.clone()
            }
        )
        .unwrap_err()
        .to_string(),
        "unpublished"
    );
    assert_eq!(token(&f, Action::Request), first);
    assert_eq!(snapshot(&f.native), before);
}

#[test]
fn malformed_native_generations_and_full_selection_fail_without_writes() {
    let f = Received::new();
    let before = snapshot(&f.native);
    for action in [
        Action::Retry { generation: vec![] },
        Action::Fresh {
            generation: vec![1; 15],
        },
        Action::StopWaiting {
            generation: vec![1; 17],
        },
    ] {
        assert_eq!(
            change(&f, action).unwrap_err().to_string(),
            "requestChanged"
        );
    }
    for (group, author, id) in [
        (vec![], f.key.author.clone(), f.key.id.to_vec()),
        (f.key.group.clone(), "other".into(), f.key.id.to_vec()),
        (f.key.group.clone(), f.key.author.clone(), vec![]),
    ] {
        assert_eq!(
            f.native
                .chats
                .change_sticker_pack_request(f.peer.clone(), group, author, id, Action::Request,)
                .unwrap_err()
                .to_string(),
            "unknownMessage"
        );
    }
    assert_eq!(snapshot(&f.native), before);
}

#[test]
fn blocked_native_action_cannot_stop_or_replace_existing_intent() {
    let f = Received::new();
    let first = token(&f, Action::Request);
    f.native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&f.peer, true)
        .unwrap();
    let before = snapshot(&f.native);
    for action in [
        Action::Request,
        Action::Fresh {
            generation: first.clone(),
        },
        Action::StopWaiting {
            generation: first.clone(),
        },
    ] {
        assert_eq!(change(&f, action).unwrap_err().to_string(), "blocked");
    }
    assert_eq!(snapshot(&f.native), before);
    assert_eq!(
        f.native
            .chats
            .session
            .lock()
            .unwrap()
            .store()
            .pending_sticker_pack(&f.peer, f.pack.id())
            .unwrap()
            .unwrap()
            .token
            .to_vec(),
        first
    );
}

#[test]
fn later_core_status_failure_is_an_unavailable_projection_not_a_failed_application() {
    let f = Received::new();
    let generation = token(&f, Action::Request);
    let before = snapshot(&f.native);
    let mut store = Store::open(
        &std::path::Path::new(&f.native.dir).join("client.db"),
        &f.native.key.identity,
    )
    .unwrap();
    let wrong = dyapp_client::store::messages::MessageKey {
        id: [0; 16],
        ..f.key.clone()
    };
    let later_read = dyapp_client::sticker_requests::status(
        &mut store,
        &f.native.chats.seal_key,
        &f.peer,
        &wrong,
    );
    assert_eq!(
        super::super::sticker_request_actions::StickerPackSaveAttempt::from_status(later_read),
        Attempt::Unavailable {
            detail: "unknownMessage".into()
        }
    );
    assert_eq!(token(&f, Action::Request), generation);
    assert_eq!(snapshot(&f.native), before);
}
