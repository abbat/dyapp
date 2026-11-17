use super::chat::Person;
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::prepare_put;
use dyapp_client::session::{Error, Session};
use dyapp_client::store::messages::MessageKey;
use dyapp_client::store::Store;
use dyapp_client::PreparedEnvelope;
use dyapp_messaging::mls::Group;
use dyapp_profile::VerifiedProfile;
use rusqlite::{params, Connection};

#[tokio::test]
async fn ack_marker_failure_keeps_projection_and_retries_without_duplicate_event() {
    let (mut fixture, mut session) = Box::pin(open_ack_failure_fixture()).await;
    let (text, key) = queue_ack_failure_message(&mut fixture, &mut session).await;
    let db = fail_ack_marker(&mut fixture, session, &key).await;
    retry_ack_after_marker_recovery(&mut fixture, &db, &text, &key).await;
}

const ACK_FAILURE_SEAL: [u8; 32] = [8; 32];

struct AckFailureFixture {
    alice: Person,
    bob: Person,
    profiles: [VerifiedProfile; 2],
    path: std::path::PathBuf,
    group: Group,
}

async fn open_ack_failure_fixture() -> (AckFailureFixture, Session) {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.require_write_tokens = false;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let profiles = [
        alice
            .node
            .get_profile(&alice.identity.peer_id())
            .await
            .unwrap(),
        bob.node.get_profile(&bob_owner).await.unwrap(),
    ];
    let path = protocol_build_path("bob-ack-failure").with_extension("sqlite3");
    Store::open(&path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&ACK_FAILURE_SEAL).unwrap(), &[])
        .unwrap();
    let (group, session) = join_ack_failure_group(&mut alice, &mut bob, &profiles, &path).await;
    (
        AckFailureFixture {
            alice,
            bob,
            profiles,
            path,
            group,
        },
        session,
    )
}

async fn join_ack_failure_group(
    alice: &mut Person,
    bob: &mut Person,
    profiles: &[VerifiedProfile; 2],
    path: &std::path::Path,
) -> (Group, Session) {
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, profiles)
        .await
        .unwrap();
    let welcome = prepare_put(&welcome[0].mailbox, welcome[0].mls.clone()).unwrap();
    alice.node.put_prepared(&welcome).await.unwrap();
    let mut session =
        Session::open(Store::open(path, &bob.identity).unwrap(), &ACK_FAILURE_SEAL).unwrap();
    let (events, more) = session
        .receive_page(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            profiles,
            &ACK_FAILURE_SEAL,
        )
        .await
        .unwrap();
    assert!(!more);
    assert!(events.iter().any(|event| matches!(event, Event::Joined(_))));
    (group, session)
}

async fn queue_ack_failure_message(
    fixture: &mut AckFailureFixture,
    session: &mut Session,
) -> (PreparedEnvelope, MessageKey) {
    for _ in 0..20 {
        fixture
            .alice
            .node
            .put(&fixture.bob.mailbox(), vec![0xff])
            .await
            .unwrap();
    }
    let text = chat::message(
        &mut fixture.group,
        &fixture.alice.device,
        &Body::text_with_id([3; 16], b"committed once".to_vec()),
    )
    .unwrap();
    let text = prepare_put(&text[0].mailbox, text[0].mls.clone()).unwrap();
    fixture.alice.node.put_prepared(&text).await.unwrap();
    let (_, more) = session
        .receive_page(
            std::slice::from_mut(&mut fixture.bob.node),
            &fixture.bob.device_key,
            &fixture.profiles,
            &ACK_FAILURE_SEAL,
        )
        .await
        .unwrap();
    assert!(more);
    let key = MessageKey {
        group: fixture.group.id().to_vec(),
        author: fixture.alice.identity.peer_id(),
        id: [3; 16],
    };
    (text, key)
}

async fn fail_ack_marker(
    fixture: &mut AckFailureFixture,
    mut session: Session,
    key: &MessageKey,
) -> Connection {
    let cursor = session
        .store()
        .mailbox_cursor_for(fixture.bob.node.peer())
        .unwrap()
        .unwrap();
    let db = install_ack_marker_failure(&fixture.path);
    assert!(matches!(
        session
            .receive_page(
                std::slice::from_mut(&mut fixture.bob.node),
                &fixture.bob.device_key,
                &fixture.profiles,
                &ACK_FAILURE_SEAL,
            )
            .await,
        Err(Error::Store(_))
    ));
    drop(session);
    let store = Store::open(&fixture.path, &fixture.bob.identity).unwrap();
    assert_eq!(
        store.chat_message(key).unwrap().unwrap().text.as_deref(),
        Some(&b"committed once"[..])
    );
    assert_eq!(store.pending_acks().unwrap().len(), 1);
    assert_eq!(
        store.mailbox_cursor_for(fixture.bob.node.peer()).unwrap(),
        Some(cursor)
    );
    drop(store);
    db
}

fn install_ack_marker_failure(path: &std::path::Path) -> Connection {
    // The node ACK succeeds; only the local mark fails after the inbox projection commits.
    let db = Connection::open(path).unwrap();
    db.execute(
        "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
        params!["test_fail_ack_marker", [1u8]],
    )
    .unwrap();
    db.execute_batch(
        "CREATE TRIGGER `test_fail_ack_marker` BEFORE UPDATE OF `ack_pending` ON `inbox`
         WHEN (SELECT `value` FROM `settings` WHERE `key` = 'test_fail_ack_marker') = x'01'
         BEGIN SELECT RAISE(ABORT, 'injected ACK marker failure'); END",
    )
    .unwrap();
    db
}

async fn retry_ack_after_marker_recovery(
    fixture: &mut AckFailureFixture,
    db: &Connection,
    text: &PreparedEnvelope,
    key: &MessageKey,
) {
    db.execute(
        "UPDATE `settings` SET `value` = ?1 WHERE `key` = ?2",
        params![[0u8], "test_fail_ack_marker"],
    )
    .unwrap();
    fixture.alice.node.put_prepared(text).await.unwrap();
    let mut session = Session::open(
        Store::open(&fixture.path, &fixture.bob.identity).unwrap(),
        &ACK_FAILURE_SEAL,
    )
    .unwrap();
    let (events, more) = session
        .receive_page(
            std::slice::from_mut(&mut fixture.bob.node),
            &fixture.bob.device_key,
            &fixture.profiles,
            &ACK_FAILURE_SEAL,
        )
        .await
        .unwrap();
    assert!(!more);
    assert_eq!(events.len(), 0);
    assert_eq!(session.store().pending_acks().unwrap().len(), 0);
    assert_eq!(
        session
            .store()
            .mailbox_cursor_for(fixture.bob.node.peer())
            .unwrap(),
        None
    );
    assert_eq!(
        session.chat_message(key).unwrap().unwrap().text.as_deref(),
        Some(&b"committed once"[..])
    );
}
