use super::*;
use crate::device_directory::tests::Fixture;
use crate::{MobileSlotResolution, MobileSlotState};
use std::path::Path;
use std::sync::{atomic::Ordering, mpsc, Barrier};
use std::time::Duration;

// Exercise removal ownership separately from the real wire decision covered by core/JNI tests.
fn capability(fixture: &Fixture) -> Arc<MobileRevokedDevice> {
    let admitted = fixture.worker.open().unwrap();
    Arc::new(MobileRevokedDevice {
        slot: fixture.worker.slot.clone(),
        wrapping: Arc::clone(&fixture.worker.wrapping),
        binding: admitted.operation.binding(),
        outcome: Mutex::new(None),
    })
}

#[test]
fn concurrent_completion_removes_only_original_slot_and_retains_success() {
    let fixture = Fixture::new();
    let receipt = capability(&fixture);
    let other = fixture.registry.create().unwrap();
    let gate = Arc::new(Barrier::new(3));
    let (completed, results) = mpsc::channel();
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let gate = Arc::clone(&gate);
            let receipt = Arc::clone(&receipt);
            let completed = completed.clone();
            std::thread::spawn(move || {
                gate.wait();
                completed
                    .send(receipt.complete().map_err(|error| error.to_string()))
                    .unwrap();
            })
        })
        .collect();
    gate.wait();
    for _ in 0..2 {
        assert_eq!(
            results.recv_timeout(Duration::from_secs(5)).unwrap(),
            Ok(None)
        );
    }
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(!Path::new(&fixture.slot.slot.directory).exists());
    assert_eq!(fixture.keys.erasures.load(Ordering::SeqCst), 1);
    assert_eq!(receipt.complete().unwrap(), None);
    assert_eq!(fixture.keys.erasures.load(Ordering::SeqCst), 1);
    let MobileSlotResolution::Ready { slot, key } = fixture.registry.resolve().unwrap() else {
        panic!("another selected identity must remain ready");
    };
    assert_eq!(slot.id, other.slot.id);
    assert_eq!(key.peer_id(), other.key.peer_id());
}

#[test]
fn partial_erasure_failure_is_retained_without_implicit_retry() {
    let fixture = Fixture::new();
    let receipt = capability(&fixture);
    fixture.keys.erase_failure.store(true, Ordering::SeqCst);
    let first = receipt.complete().unwrap_err().to_string();
    assert!(first.contains("fixture erasure refused"));
    fixture.keys.erase_failure.store(false, Ordering::SeqCst);
    assert_eq!(receipt.complete().unwrap_err().to_string(), first);
    assert_eq!(fixture.keys.erasures.load(Ordering::SeqCst), 1);
    let inventory = fixture.registry.list().unwrap();
    let original = inventory
        .iter()
        .find(|slot| slot.id == fixture.slot.slot.id)
        .unwrap();
    assert_eq!(original.state, MobileSlotState::Deleting);
}

#[test]
fn replaced_context_refusal_is_terminal_even_if_original_blob_returns() {
    let fixture = Fixture::new();
    let receipt = capability(&fixture);
    let path = Path::new(&fixture.slot.slot.directory).join("device.key");
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, [1]).unwrap();
    let first = receipt.complete().unwrap_err().to_string();
    std::fs::write(&path, original).unwrap();
    assert_eq!(receipt.complete().unwrap_err().to_string(), first);
    assert_eq!(fixture.keys.erasures.load(Ordering::SeqCst), 0);
    assert!(fixture.worker.devices(false).is_ok());
}

#[test]
fn failed_probe_preserves_original_credentials_and_issues_no_capability() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.worker.revoked_device().err().unwrap().to_string(),
        "notConnected"
    );
    assert_eq!(fixture.keys.erasures.load(Ordering::SeqCst), 0);
    assert!(fixture.worker.devices(false).is_ok());
}
