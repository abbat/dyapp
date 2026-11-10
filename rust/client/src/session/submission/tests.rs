mod interactions;
mod media;

use super::*;
use crate::store::messages::MessageKey;
use crate::store::Store;
use dyapp_identity::Identity;
use dyapp_messaging::mls::Device;
use dyapp_p2p_net::proto::Status;
use dyapp_profile::Profile;
use std::path::PathBuf;

struct Fixture {
    session: Session,
    owner: Identity,
    profiles: Vec<VerifiedProfile>,
    peer: String,
    group: Vec<u8>,
    path: PathBuf,
    seal: [u8; 32],
}

fn person() -> (Identity, Device, VerifiedProfile) {
    let owner = Identity::generate();
    let device = Device::new(&owner, Identity::generate().public_key(), 1).unwrap();
    let profile = Profile {
        version: 1,
        devices: vec![device.certificate().clone()],
        ..Profile::default()
    };
    let verified = dyapp_profile::verify(&profile.sign(&owner)).unwrap();
    (owner, device, verified)
}

fn fixture() -> Fixture {
    let (owner, device, own) = person();
    let (_, peer_device, peer) = person();
    let profiles = vec![own, peer.clone()];
    let (group, _) = device
        .create_group(&[peer_device.key_package(false).unwrap()], &profiles)
        .unwrap();
    let seal = [7; 32];
    let path = crate::store::tests::temp_db_path();
    let mut store = Store::open(&path, &owner).unwrap();
    store
        .stage_group(&group, &peer, &device.seal(&seal).unwrap(), &[])
        .unwrap();
    Fixture {
        session: Session::open(store, &seal).unwrap(),
        owner,
        profiles,
        peer: peer.peer_id,
        group: group.id().to_vec(),
        path,
        seal,
    }
}

async fn queued_text(f: &mut Fixture) -> [u8; 16] {
    let result = f
        .session
        .submit_text_to(&mut [], &f.profiles, b"offline", &f.seal)
        .await
        .unwrap();
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

fn writes(session: &Session) -> Vec<Vec<u8>> {
    let mut bytes: Vec<_> = session
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .map(|write| write.as_bytes().to_vec())
        .collect();
    bytes.sort();
    bytes
}

#[tokio::test]
async fn queued_text_keeps_its_identity_and_exact_writes_across_reopen_and_retry() {
    let mut f = fixture();
    let id = queued_text(&mut f).await;
    let key = MessageKey {
        group: f.group.clone(),
        author: f.owner.peer_id(),
        id,
    };
    let before = writes(&f.session);
    assert_ne!(before, [] as [Vec<u8>; 0]);
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    assert_eq!(
        reopened
            .chat_message(&key)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(b"offline".as_slice())
    );
    assert_eq!(writes(&reopened), before);
    // An empty node list is a deterministic transport failure, with a fixed retry bookkeeping clock.
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), before);
    let rows = reopened.store().current_chat_messages(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
}

#[tokio::test]
async fn queued_edit_updates_only_the_target_and_returns_its_operation_id() {
    let mut f = fixture();
    let id = queued_text(&mut f).await;
    let outcome = f
        .session
        .submit_edit(&mut [], &f.group, id, b"revised", &f.seal)
        .await
        .unwrap();
    assert_ne!(outcome.id(), id);
    assert!(matches!(outcome, Submission::Queued { .. }));
    let rows = f.session.store().current_chat_messages(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    assert_eq!(rows[0].current.text.as_deref(), Some(b"revised".as_slice()));
    assert!(rows[0].current.edited);
}

#[tokio::test]
async fn queued_delete_is_terminal_and_a_stale_edit_never_creates_a_message() {
    let mut f = fixture();
    let id = queued_text(&mut f).await;
    let outcome = f
        .session
        .submit_delete(&mut [], &f.group, id, &f.seal)
        .await
        .unwrap();
    assert_ne!(outcome.id(), id);
    assert!(matches!(outcome, Submission::Queued { .. }));
    let key = MessageKey {
        group: f.group.clone(),
        author: f.owner.peer_id(),
        id,
    };
    assert!(f.session.chat_message(&key).unwrap().unwrap().deleted);
    let before = writes(&f.session);
    assert!(matches!(
        f.session
            .submit_edit(&mut [], &f.group, id, b"restore", &f.seal)
            .await,
        Err(Error::UnknownMessage)
    ));
    assert!(matches!(
        f.session
            .submit_delete(&mut [], &f.group, id, &f.seal)
            .await,
        Err(Error::UnknownMessage)
    ));
    assert_eq!(writes(&f.session), before);
    assert_eq!(
        f.session
            .store()
            .current_chat_messages(&f.peer)
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn failure_before_commit_reports_no_submission_and_preserves_the_snapshot() {
    let mut f = fixture();
    let sealed = f.session.store().sealed_device().unwrap();
    rusqlite::Connection::open(&f.path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
         BEGIN SELECT RAISE(ABORT, 'test failure'); END",
        )
        .unwrap();
    assert!(matches!(
        f.session
            .submit_text_to(&mut [], &f.profiles, b"not saved", &f.seal)
            .await,
        Err(Error::Store(_))
    ));
    assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
    assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
    assert!(f
        .session
        .store()
        .current_chat_messages(&f.peer)
        .unwrap()
        .is_empty());
    assert!(f.session.device.is_none());
}

#[tokio::test]
async fn strict_edit_and_delete_keep_committed_changes_on_delivery_failure() {
    let mut f = fixture();
    let target = queued_text(&mut f).await;
    assert!(matches!(
        f.session
            .send_edit(&mut [], &f.group, target, b"changed", &f.seal)
            .await,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    let rows = f.session.store().current_chat_messages(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, target);
    assert!(rows[0].current.edited);
    assert_eq!(rows[0].current.text.as_deref(), Some(b"changed".as_slice()));
    let edited_writes = writes(&f.session);
    assert!(matches!(
        f.session
            .send_delete(&mut [], &f.group, target, &f.seal)
            .await,
        Err(Error::Client(crate::Error::Status(Status::NotFound)))
    ));
    let rows = f.session.store().current_chat_messages(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, target);
    assert!(rows[0].current.deleted);
    assert_eq!(rows[0].current.text, None);
    let deleted_writes = writes(&f.session);
    assert!(deleted_writes.len() > edited_writes.len());
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), deleted_writes);
    assert!(
        reopened.store().current_chat_messages(&f.peer).unwrap()[0]
            .current
            .deleted
    );
}

#[tokio::test]
async fn strict_send_still_reports_delivery_failure_after_a_single_committed_text() {
    let mut f = fixture();
    assert!(matches!(
        f.session
            .send_text_to(&mut [], &f.profiles, b"offline", &f.seal)
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
}
