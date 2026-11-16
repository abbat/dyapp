use super::*;
use crate::identity_slots::tests::{link::WrappedKeys, root};
use crate::identity_slots::{Layout, Registry, Slot, Storage};
use crate::link::Linked;
use dyapp_identity::Identity;

struct Fixture {
    registry: Registry,
    keys: WrappedKeys,
    source: Slot,
    peer: String,
    view: ProfileView,
    receiver: LinkReceiver<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let registry = Registry::new(root(), Layout::Android);
        let keys = WrappedKeys::default();
        let (source, owner, _) = registry.create(Storage::Wrapped, &keys).unwrap();
        let peer = owner.peer_id();
        let view = ProfileView::default();
        let receiver = {
            let guard = view.current().unwrap();
            let invitation = registry
                .begin_link(&source.id, &peer, false, &keys, 100)
                .unwrap();
            LinkReceiver::new(invitation, guard.generation()).unwrap()
        };
        Self {
            registry,
            keys,
            source,
            peer,
            view,
            receiver,
        }
    }

    // Controlled network handoff; the production installer still performs every durable write.
    fn received(&self) -> super::super::LinkPoll<'_> {
        let poll = self.receiver.admission.receive().unwrap();
        self.receiver.state().unwrap().received = Some(ReceivedLink::new(
            &self.registry,
            self.source.id.clone(),
            self.peer.clone(),
            false,
            Linked {
                identity: Identity::generate(),
                state: b"received state".to_vec(),
            },
            Identity::generate(),
        ));
        poll
    }

    async fn unchanged(&self, phase: LinkPhase) {
        assert_eq!(
            self.receiver
                .poll_with(
                    &self.keys,
                    &self.view,
                    100,
                    |_| async { panic!("terminal receiver dialed again") },
                    |_| panic!("terminal receiver wrote again")
                )
                .await
                .unwrap(),
            phase
        );
    }
}

#[tokio::test]
async fn persisted_success_is_retained_by_the_client_package_receipt() {
    let fixture = Fixture::new();
    assert_eq!(fixture.receiver.result().unwrap(), None);
    let write = fixture
        .received()
        .install(&fixture.view, &fixture.receiver.generation)
        .unwrap();
    assert_eq!(
        fixture
            .receiver
            .install(write, |received| {
                received
                    .install(&fixture.keys)
                    .map(|(slot, _, _)| slot.id.clone())
                    .map_err(ToString::to_string)
            })
            .unwrap(),
        LinkPhase::Completed
    );
    let result = fixture.receiver.result().unwrap().unwrap().unwrap();
    let slots = fixture.registry.list().unwrap();
    assert_eq!(slots.len(), 2);
    assert!(slots
        .iter()
        .any(|slot| slot.id == result && slot.id != fixture.source.id));
    fixture.unchanged(LinkPhase::Completed).await;
    assert_eq!(fixture.receiver.result().unwrap(), Some(Ok(result)));
}

#[tokio::test]
async fn failed_installation_retains_its_original_receipt_without_retrying() {
    let fixture = Fixture::new();
    let write = fixture
        .received()
        .install(&fixture.view, &fixture.receiver.generation)
        .unwrap();
    assert_eq!(
        fixture
            .receiver
            .install(write, |received| {
                received
                    .install_with(&fixture.keys, &|_, _| Err("platform unavailable".into()))
                    .map(|(slot, _, _)| slot.id.clone())
                    .map_err(ToString::to_string)
            })
            .unwrap(),
        LinkPhase::Failed
    );
    assert_eq!(
        fixture.receiver.result().unwrap(),
        Some(Err("platform unavailable".into()))
    );
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
    fixture.unchanged(LinkPhase::Failed).await;
    assert_eq!(
        fixture.receiver.result().unwrap(),
        Some(Err("platform unavailable".into()))
    );
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
}

#[tokio::test]
async fn callback_unwind_keeps_interrupted_receiving_state_without_an_empty_success_receipt() {
    let fixture = Fixture::new();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let write = fixture
            .received()
            .install(&fixture.view, &fixture.receiver.generation)
            .unwrap();
        fixture.receiver.install(write, |received| {
            received
                .install_with(&fixture.keys, &|_, _| panic!("controlled callback unwind"))
                .map(|(slot, _, _)| slot.id.clone())
                .map_err(ToString::to_string)
        })
    }));
    assert!(unwound.is_err());
    assert_eq!(fixture.receiver.phase(), LinkPhase::Interrupted);
    assert!(fixture.receiver.result().is_err());
    fixture.unchanged(LinkPhase::Interrupted).await;
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
}
