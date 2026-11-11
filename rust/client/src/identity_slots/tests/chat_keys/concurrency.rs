use super::*;
use std::sync::mpsc;
use std::time::Duration;

struct Pausing {
    inner: Wrapping,
    progress: mpsc::Sender<ChatKeyKind>,
    permit: mpsc::Receiver<()>,
}

impl KeyReader for Pausing {
    fn contains(&self, slot: &Slot) -> Result<bool, String> {
        self.inner.contains(slot)
    }

    fn open(&self, slot: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        self.inner.open(slot, wrapped)
    }
}

impl ChatKeyStore for Pausing {
    fn wrap(&self, slot: &Slot, kind: ChatKeyKind, secret: &[u8; 32]) -> Result<Vec<u8>, String> {
        self.progress
            .send(kind)
            .map_err(|error| error.to_string())?;
        self.permit
            .recv_timeout(Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        self.inner.wrap(slot, kind, secret)
    }

    fn unwrap(
        &self,
        slot: &Slot,
        kind: ChatKeyKind,
        bytes: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        self.inner.unwrap(slot, kind, bytes)
    }
}

fn start_bootstrap(
    registry: Registry,
    wrapping: Wrapping,
    progress: mpsc::Sender<ChatKeyKind>,
    permit: mpsc::Receiver<()>,
    ready: mpsc::Sender<Result<crate::identity_slots::ChatOperation, String>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let peer = wrapping.owner.peer_id();
        let wrapping = Pausing {
            inner: wrapping,
            progress,
            permit,
        };
        assert!(ready
            .send(registry.chat_operation(FIRST, &peer, &wrapping, &wrapping))
            .is_ok());
    })
}

#[test]
fn callback_signals_prove_exclusive_bootstrap_then_shared_operation_admission() {
    let (dir, registry, wrapping) = fixture(Layout::Android);
    let lock = wrapping.lock.clone();
    let (progress, observe) = mpsc::channel();
    let (permit, proceed) = mpsc::channel();
    let (ready, admitted) = mpsc::channel();
    let worker = start_bootstrap(registry, wrapping, progress, proceed, ready);
    for kind in [ChatKeyKind::Device, ChatKeyKind::State] {
        assert_eq!(
            observe
                .recv_timeout(Duration::from_secs(5))
                .expect("bootstrap callback"),
            kind
        );
        assert!(matches!(
            handle(&lock).try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
        permit.send(()).unwrap();
    }
    let operation = admitted
        .recv_timeout(Duration::from_secs(5))
        .expect("shared admission")
        .unwrap();
    worker.join().unwrap();
    assert!(matches!(
        handle(&lock).try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    handle(&lock)
        .try_lock_shared()
        .expect("admitted operation permits reads");
    assert!(matches!(
        handle(&dir.join("chat.lock")).try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(operation);
    handle(&lock)
        .try_lock()
        .expect("completed operation permits mutations");
    handle(&dir.join("chat.lock"))
        .try_lock()
        .expect("completed chat releases serialization");
}
