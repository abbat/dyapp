use super::*;
use std::future::{pending, Future};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

fn mutation_guard(root: &std::path::Path) -> fs::File {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("profiles.lock"))
        .unwrap()
}

#[test]
fn idle_invitation_has_a_parseable_code_and_no_operation_lease() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (slot, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let waiting = registry
        .begin_link(&slot.id, &owner.peer_id(), false, &Files, 100)
        .unwrap();
    let code = waiting.code().unwrap();
    assert_eq!(code.expires_at, 400);
    assert_eq!(crate::link::Code::parse(&code.text(), 100).unwrap(), code);
    mutation_guard(&root).try_lock().unwrap();
    assert_eq!(registry.list().unwrap(), vec![slot]);
    drop(waiting);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invitation_creation_refuses_wrong_owner_selection_and_overflow_without_writes() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    assert!(registry
        .begin_link(&source.id, "wrong owner", false, &Files, 100)
        .is_err());
    assert!(registry
        .begin_link(&source.id, &owner.peer_id(), false, &Files, u64::MAX)
        .is_err());
    let (other, _, _) = registry.create(Storage::File, &Files).unwrap();
    assert!(registry
        .begin_link(&source.id, &owner.peer_id(), false, &Files, 100)
        .is_err());
    assert_eq!(registry.list().unwrap().len(), 2);
    assert_eq!(active(&root, Layout::Desktop), Active::Selected(other.id));
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn expired_and_changed_source_polling_never_dials() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let mut waiting = registry
        .begin_link(&source.id, &owner.peer_id(), false, &Files, 100)
        .unwrap();
    let expiry = waiting.code().unwrap().expires_at;
    let result = waiting
        .poll_with(&Files, expiry, |_| async { panic!("expired dial") })
        .await;
    assert!(matches!(result, Err(error) if error == "bad client input: the link code expired"));
    let mut waiting = registry
        .begin_link(&source.id, &owner.peer_id(), false, &Files, 100)
        .unwrap();
    registry.create(Storage::File, &Files).unwrap();
    let result = waiting
        .poll_with(&Files, 100, |_| async { panic!("stale dial") })
        .await;
    assert!(matches!(result, Err(error) if error == "The link source changed"));
    mutation_guard(&root).try_lock().unwrap();
    drop(waiting);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pending_connection_uses_active_network_and_holds_only_its_operation_lease() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    let network = Network {
        name: "invitation".into(),
        seeds: vec!["127.0.0.1:3210".into()],
    };
    network::save(&source.dir, Settings::Active, &network).unwrap();
    let mut waiting = registry
        .begin_link(&source.id, &owner.peer_id(), false, &Files, 100)
        .unwrap();
    let connected = Cell::new(false);
    {
        let future = waiting.poll_with(&Files, 100, |actual| {
            assert_eq!(actual, network);
            connected.set(true);
            pending()
        });
        let mut future = pin!(future);
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        assert!(connected.get());
        assert!(mutation_guard(&root).try_lock().is_err());
    }
    mutation_guard(&root).try_lock().unwrap();
    assert!(waiting.code().is_some());
    drop(waiting);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn a_transient_connection_failure_keeps_the_same_code_and_releases_admission() {
    let root = root();
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
    network::save(
        &source.dir,
        Settings::Active,
        &Network {
            name: "invitation".into(),
            seeds: vec!["127.0.0.1:3210".into()],
        },
    )
    .unwrap();
    let mut waiting = registry
        .begin_link(&source.id, &owner.peer_id(), false, &Files, 100)
        .unwrap();
    let code = waiting.code();
    let result = waiting
        .poll_with(&Files, 100, |_| async { Err("offline".into()) })
        .await;
    assert!(matches!(result, Err(error) if error == "offline"));
    assert_eq!(waiting.code(), code);
    mutation_guard(&root).try_lock().unwrap();
    drop(waiting);
    fs::remove_dir_all(root).unwrap();
}
