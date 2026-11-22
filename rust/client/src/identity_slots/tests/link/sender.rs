use super::*;
use crate::link::{Code, Invite};
use std::future::{pending, Future};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

struct Fixture {
    root: std::path::PathBuf,
    registry: Registry,
    source: Slot,
    peer: String,
    view: ProfileView,
    code: Code,
    _test_dir: dyapp_test_support::TestDir,
}

impl Fixture {
    fn new() -> Self {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let root = test_dir.to_path_buf();
        let registry = Registry::new(root.clone(), Layout::Desktop);
        let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
        let peer = owner.peer_id();
        Self {
            root,
            registry,
            source,
            peer,
            view: ProfileView::default(),
            code: *Invite::new([7; 32], 100).unwrap().code(),
            _test_dir: test_dir,
        }
    }

    fn sender(&self) -> LinkSender {
        self.registry
            .prepare_link_sender(
                &self.view,
                &self.source.id,
                &self.peer,
                &self.code.text(),
                &Files,
                100,
            )
            .unwrap()
    }

    fn lock(&self) -> fs::File {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.root.join("profiles.lock"))
            .unwrap()
    }
}

#[tokio::test]
async fn prepared_targets_hold_no_idle_lease_and_stale_aba_approval_never_dials() {
    let fixture = Fixture::new();
    let sender = fixture.sender();
    assert_eq!(sender.code(), fixture.code);
    fixture.lock().try_lock().unwrap();
    {
        let _view = fixture.view.transition().unwrap();
        fixture.registry.create(Storage::File, &Files).unwrap();
    }
    {
        let _view = fixture.view.transition().unwrap();
        fixture.registry.select(&fixture.source.id, &Files).unwrap();
    }
    let result = sender
        .confirm_and_send_with(&Files, &fixture.view, 100, |_| async {
            panic!("stale approval dialed")
        })
        .await;
    assert!(matches!(result, Err(error) if error == "Profile view changed"));
    assert!(!fixture.source.dir.join("client.db").exists());
    fixture.lock().try_lock().unwrap();
}

#[tokio::test]
async fn expiry_and_external_selection_change_refuse_transfer_before_snapshot_or_network() {
    let fixture = Fixture::new();
    let sender = fixture.sender();
    let result = sender
        .confirm_and_send_with(&Files, &fixture.view, fixture.code.expires_at, |_| async {
            panic!("expired approval dialed")
        })
        .await;
    assert!(matches!(result, Err(error) if error == "expired"));
    fixture.registry.create(Storage::File, &Files).unwrap();
    let result = sender
        .confirm_and_send_with(&Files, &fixture.view, 100, |_| async {
            panic!("changed source dialed")
        })
        .await;
    assert!(matches!(result, Err(error) if error == "The link source changed"));
    assert!(!fixture.source.dir.join("client.db").exists());
    fixture.lock().try_lock().unwrap();
}

#[test]
fn actual_pending_delivery_retains_its_operation_and_releases_it_on_worker_drop() {
    let fixture = Fixture::new();
    let sender = fixture.sender();
    let network = Network {
        name: "owner".into(),
        seeds: vec!["127.0.0.1:3210".into()],
    };
    network::save(&fixture.source.dir, Settings::Active, &network).unwrap();
    let connected = Cell::new(false);
    {
        let future = sender.confirm_and_send_with(&Files, &fixture.view, 100, |actual| {
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
        assert!(matches!(
            fixture.lock().try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
    }
    fixture.lock().try_lock().unwrap();
    assert_eq!(fixture.registry.list().unwrap().len(), 1);
}
