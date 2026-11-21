//! Reuse the actual profile responder; no production fetch hook or timing assumption.
use crate::link_review::tests::server;

use super::*;
use std::fs::{OpenOptions, TryLockError};
use std::sync::mpsc;
use std::time::Duration;

struct HeldProfile {
    entered: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

impl HeldProfile {
    fn install(server: &server::Server) -> Self {
        let (entered, observed) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        *server.on_read.lock().unwrap() = Some(Box::new(move || {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
        }));
        Self {
            entered: observed,
            release,
        }
    }
}

impl Drop for HeldProfile {
    fn drop(&mut self) {
        let _ = self.release.send(());
    }
}

fn live_server(fixture: &Fixture) -> server::Server {
    let remote = Profile {
        version: fixture.cached.version + 1,
        ..Default::default()
    };
    let server = server::Server::new(remote.sign(&fixture.slot.key.identity));
    dyapp_identity::network::save(
        PathBuf::from(&fixture.slot.slot.directory).as_path(),
        dyapp_identity::network::Settings::Active,
        &server.network,
    )
    .unwrap();
    server
}

#[test]
fn actual_profile_reply_retains_admission_and_never_overwrites_the_editor() {
    let fixture = Fixture::new();
    let server = live_server(&fixture);
    let held = HeldProfile::install(&server);
    let worker = Arc::clone(&fixture.worker);
    let (completed, result) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let result = worker.snapshot(true);
        drop(worker);
        completed.send(result).unwrap();
    });
    held.entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(&fixture.slot.slot.directory).join("chat.lock"))
        .unwrap();
    assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
    assert_eq!(
        fixture.store().profile_draft().unwrap(),
        Some(fixture.cached.clone())
    );
    drop(held);
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap(),
        MobileDeviceSnapshot {
            devices: vec![],
            state: MobileDeviceReadState::Verified,
            source: MobileDeviceRowSource::Remote,
        }
    );
    reader.join().unwrap();
    lock.try_lock().unwrap();
    lock.unlock().unwrap();
    unchanged(&fixture, &server);
    // Keeping the idle worker alive must not retain admission after the completed network call.
    fixture.registry.create().unwrap();
}

#[test]
fn verified_older_readback_keeps_saved_rows_with_explicit_source() {
    let fixture = Fixture::new();
    let server = live_server(&fixture);
    *server.record.lock().unwrap() = Profile {
        version: fixture.cached.version - 1,
        ..Profile::default()
    }
    .sign(&fixture.slot.key.identity);
    let saved = fixture.worker.snapshot(false).unwrap().devices;
    assert_eq!(
        fixture.worker.snapshot(true).unwrap(),
        MobileDeviceSnapshot {
            devices: saved,
            state: MobileDeviceReadState::Verified,
            source: MobileDeviceRowSource::Saved,
        }
    );
    unchanged(&fixture, &server);
}

fn unchanged(fixture: &Fixture, server: &server::Server) {
    assert_eq!(
        fixture.store().profile_draft().unwrap(),
        Some(fixture.cached.clone())
    );
    assert_eq!(
        server
            .profile_writes
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(*server.packages.records.lock().unwrap(), vec![]);
    assert_eq!(
        server.record.lock().unwrap().public_key,
        fixture.slot.key.identity.public_key()
    );
    assert_eq!(
        fixture
            .keys
            .erasures
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}
