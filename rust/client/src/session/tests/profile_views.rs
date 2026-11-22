use super::*;
use crate::interactions::Kind;
use crate::store::Conversation;

const VIEW_TIME: u64 = 1_700_000_000_000;

fn packages(invite: &Invite) -> Vec<(Vec<u8>, Vec<u8>)> {
    let device: [u8; 32] = invite.alice.device_key().unwrap().try_into().unwrap();
    vec![(
        crate::mailbox(&device).to_vec(),
        invite.alice.key_package(false).unwrap(),
    )]
}

fn unchanged(session: &Session, sealed: &Option<Vec<u8>>) {
    assert_eq!(&session.store.sealed_device().unwrap(), sealed);
    assert!(session.store.known_groups().unwrap().is_empty());
    assert!(session.due_outbound(i64::MAX).unwrap().is_empty());
}

#[tokio::test]
async fn a_disabled_first_view_does_not_fetch_keys_or_change_mls() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let seal = [5; 32];
    let mut invite = bob_invited(&dir.path().join("session-1.db"), &seal);
    let before = invite.session.store.sealed_device().unwrap();
    assert!(invite
        .session
        .submit_interaction_with(&mut [], &invite.profiles, Kind::View, &seal)
        .await
        .unwrap()
        .is_none());
    unchanged(&invite.session, &before);
}

#[test]
fn a_first_view_commits_welcome_and_signal_without_a_like_or_chat_request() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let seal = [5; 32];
    let path = dir.path().join("session-1.db");
    let mut invite = bob_invited(&path, &seal);
    let packages = packages(&invite);
    let peer = invite.alice_id.peer_id();
    invite.session.store.set_send_profile_views(true).unwrap();
    let (event, writes) = invite
        .session
        .stage_first_view(&invite.profiles, &packages, VIEW_TIME, &seal)
        .unwrap()
        .unwrap();
    assert_eq!(event.kind, Kind::View);
    assert_eq!(Timestamp::from(event.hlc).wall_ms(), VIEW_TIME);
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].envelope().class, EnvelopeClass::Message as i32);
    assert_eq!(writes[1].envelope().class, EnvelopeClass::Signal as i32);
    assert!(invite
        .session
        .store
        .viewed_on(&invite.bob_id.peer_id(), &peer, VIEW_TIME)
        .unwrap());
    assert!(!invite.session.likes(&peer).unwrap().matched());
    assert!(!invite.session.likes(&peer).unwrap().ours);
    assert_eq!(
        invite.session.store.conversation(&peer).unwrap(),
        Conversation::None
    );
    assert!(invite.session.store.chat_rows().unwrap().is_empty());

    let mut receiver_store =
        Store::open(&dir.path().join("receiver.db"), &invite.alice_id).unwrap();
    receiver_store
        .stage_outbound(&invite.alice.seal(&seal).unwrap(), &[])
        .unwrap();
    let mut receiver = Session::open(receiver_store, &seal).unwrap();
    let waiting = crate::mail_notices::capture(receiver.store()).unwrap();
    for write in &writes {
        let key = InboxKey {
            mailbox: *write.mailbox(),
            id: *write.id(),
        };
        receiver
            .stage_envelope(&key, &write.envelope().ciphertext, &invite.profiles, &seal)
            .unwrap();
    }
    let sender = invite.bob_id.peer_id();
    assert_received_view(&receiver, &sender, &peer, &waiting);

    let before: Vec<_> = writes
        .iter()
        .map(|write| write.as_bytes().to_vec())
        .collect();
    let group = invite.session.pair_group(&peer).unwrap();
    drop(invite.session);
    let mut reopened = Session::open(Store::open(&path, &invite.bob_id).unwrap(), &seal).unwrap();
    assert_eq!(reopened.pair_group(&peer), Some(group));
    let mut saved: Vec<_> = reopened
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .map(|write| write.as_bytes().to_vec())
        .collect();
    saved.sort();
    let mut expected = before;
    expected.sort();
    assert_eq!(saved, expected);
    let sealed = reopened.store.sealed_device().unwrap();
    assert!(reopened
        .stage_first_view(&invite.profiles, &[], VIEW_TIME, &seal)
        .unwrap()
        .is_none());
    assert_eq!(reopened.store.sealed_device().unwrap(), sealed);
    let mut after: Vec<_> = reopened
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .map(|write| write.as_bytes().to_vec())
        .collect();
    after.sort();
    assert_eq!(after, expected);
}

fn assert_received_view(
    receiver: &Session,
    sender: &str,
    peer: &str,
    before: &crate::mail_notices::Waiting,
) {
    assert!(!receiver.store.send_profile_views().unwrap());
    assert!(receiver.store.viewed_on(sender, peer, VIEW_TIME).unwrap());
    assert_eq!(
        receiver.store.received_profile_views().unwrap(),
        vec![crate::store::ReceivedProfileView {
            peer_id: sender.to_owned(),
            utc_date: "2023-11-14".into(),
        }]
    );
    assert!(!receiver.likes(sender).unwrap().ours);
    assert!(!receiver.likes(sender).unwrap().theirs);
    assert_eq!(
        receiver.store.conversation(sender).unwrap(),
        Conversation::None
    );
    assert!(receiver.store.chat_rows().unwrap().is_empty());
    let after = crate::mail_notices::capture(receiver.store()).unwrap();
    assert!(
        crate::mail_notices::decide(before, &after, true, crate::store::NoticeContent::Text,)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_blocked_first_view_refuses_before_key_package_io() {
    let dir = dyapp_test_support::test_dir().unwrap();
    let seal = [5; 32];
    let mut invite = bob_invited(&dir.path().join("session-1.db"), &seal);
    invite.session.store.set_send_profile_views(true).unwrap();
    invite
        .session
        .store
        .set_blocked(&invite.alice_id.peer_id(), true)
        .unwrap();
    let before = invite.session.store.sealed_device().unwrap();
    assert!(matches!(
        invite
            .session
            .submit_interaction_with(&mut [], &invite.profiles, Kind::View, &seal)
            .await,
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
    unchanged(&invite.session, &before);
}

#[test]
fn first_view_failure_rolls_back_the_invitation_signal_event_and_day() {
    for (table, operation) in [
        ("interaction_ledger", "INSERT"),
        ("view_days", "INSERT"),
        ("conversation_groups", "INSERT"),
        ("mls_state", "UPDATE"),
        ("outbox", "INSERT"),
    ] {
        let dir = dyapp_test_support::test_dir().unwrap();
        let seal = [5; 32];
        let path = dir.path().join("session-1.db");
        let mut invite = bob_invited(&path, &seal);
        let packages = packages(&invite);
        invite.session.store.set_send_profile_views(true).unwrap();
        let before = invite.session.store.sealed_device().unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        // Both identifiers come only from the in-code constant cases above.
        connection
            .execute_batch(&format!(
                "CREATE TRIGGER `fail_view` BEFORE {operation} ON `{table}`
             BEGIN SELECT RAISE(ABORT, 'controlled first-view refusal'); END"
            ))
            .unwrap();
        assert!(
            invite
                .session
                .stage_first_view(&invite.profiles, &packages, VIEW_TIME, &seal)
                .is_err(),
            "{table}"
        );
        unchanged(&invite.session, &before);
        assert!(invite.session.device.is_none());
        assert!(!invite
            .session
            .store
            .viewed_on(
                &invite.bob_id.peer_id(),
                &invite.alice_id.peer_id(),
                VIEW_TIME
            )
            .unwrap());
        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM `interaction_ledger`", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(events, 0, "{table}");
    }
}
