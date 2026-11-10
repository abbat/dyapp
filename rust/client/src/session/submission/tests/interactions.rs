use super::*;
use crate::interactions::{Interaction, Kind};

fn persisted_event(f: &Fixture, event: Interaction) -> Interaction {
    let payload: Vec<u8> = rusqlite::Connection::open(&f.path)
        .unwrap()
        .query_row(
            "SELECT `payload` FROM `interaction_ledger` WHERE `sender` = ?1 AND `event_id` = ?2",
            rusqlite::params![f.owner.peer_id(), event.event_id.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    Interaction::decode(&payload).unwrap()
}

fn unjoined(f: &Fixture) -> Session {
    Session::open_or_create(
        Store::open(&crate::store::tests::temp_db_path(), &f.owner).unwrap(),
        &f.owner,
        &Identity::generate(),
        &f.seal,
    )
    .unwrap()
}

#[tokio::test]
async fn queued_like_retains_the_actual_event_and_exact_retry_bytes() {
    let mut f = fixture();
    let committed = f
        .session
        .submit_interaction_with(&mut [], &f.profiles, Kind::Like, &f.seal)
        .await
        .unwrap()
        .unwrap();
    let event = committed.event;
    assert_eq!(event.kind, Kind::Like);
    assert!(matches!(
        committed.delivery,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    assert_eq!(persisted_event(&f, event), event);
    assert!(f.session.store().likes(&f.peer).unwrap().ours);
    let before = writes(&f.session);
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), before);
    assert!(reopened.store().likes(&f.peer).unwrap().ours);
}

#[tokio::test]
async fn a_matching_old_like_does_not_turn_a_precommit_refusal_into_a_submission() {
    let mut f = fixture();
    f.session
        .submit_interaction(&mut [], &f.group, Kind::Like, &f.seal)
        .await
        .unwrap()
        .unwrap();
    let before = writes(&f.session);
    let sealed = f.session.store().sealed_device().unwrap();
    rusqlite::Connection::open(&f.path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER `fail_signal` BEFORE UPDATE ON `mls_state`
         BEGIN SELECT RAISE(ABORT, 'test failure'); END",
        )
        .unwrap();
    assert!(matches!(
        f.session
            .submit_interaction_with(&mut [], &f.profiles, Kind::Like, &f.seal)
            .await,
        Err(Error::Store(_))
    ));
    assert!(f.session.store().likes(&f.peer).unwrap().ours);
    assert_eq!(writes(&f.session), before);
    assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
    assert!(f.session.device.is_none());
}

#[tokio::test]
async fn blocked_and_unknown_group_refusals_commit_no_interaction() {
    let mut f = fixture();
    f.session.store().set_blocked(&f.peer, true).unwrap();
    let sealed = f.session.store().sealed_device().unwrap();
    assert!(matches!(
        f.session
            .submit_interaction_with(&mut [], &f.profiles, Kind::Like, &f.seal)
            .await,
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
    assert!(matches!(
        f.session
            .submit_interaction(&mut [], b"missing group", Kind::Like, &f.seal)
            .await,
        Err(Error::UnknownGroup)
    ));
    assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
    assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
    assert!(!f.session.store().likes(&f.peer).unwrap().ours);
}

#[tokio::test]
async fn disabled_view_is_no_event_and_preserves_mls() {
    let mut f = fixture();
    f.session.store().set_send_profile_views(false).unwrap();
    let sealed = f.session.store().sealed_device().unwrap();
    assert!(f
        .session
        .submit_interaction(&mut [], &f.group, Kind::View, &f.seal)
        .await
        .unwrap()
        .is_none());
    assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
    assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
}

#[tokio::test]
async fn an_unjoined_unlike_is_no_event_and_first_like_refusal_stages_nothing() {
    let f = fixture();
    let mut session = unjoined(&f);
    let sealed = session.store().sealed_device().unwrap();
    assert!(session
        .submit_interaction_with(&mut [], &f.profiles, Kind::Unlike, &f.seal)
        .await
        .unwrap()
        .is_none());
    assert!(matches!(
        session
            .submit_interaction_with(&mut [], &f.profiles, Kind::Like, &f.seal)
            .await,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    assert_eq!(writes(&session), [] as [Vec<u8>; 0]);
    assert_eq!(session.store().sealed_device().unwrap(), sealed);
    assert!(!session.store().likes(&f.peer).unwrap().ours);
}

#[tokio::test]
async fn strict_interaction_calls_preserve_their_delivery_failure_contract() {
    let mut f = fixture();
    assert!(matches!(
        f.session
            .interact_with(&mut [], &f.profiles, Kind::Like, &f.seal)
            .await,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    assert!(f.session.store().likes(&f.peer).unwrap().ours);
    assert!(matches!(
        f.session
            .interact(&mut [], &f.group, Kind::Unlike, &f.seal)
            .await,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    assert!(!f.session.store().likes(&f.peer).unwrap().ours);
}
