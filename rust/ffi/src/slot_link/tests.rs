mod receipts;
mod wire;

use super::*;
use crate::slot_registry_tests::{root, Credentials};
use crate::{MobileSlotLayout, MobileSlotSelection};
use dyapp_client::identity_slots::{Layout, Registry};
use dyapp_identity::Identity;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    registry: Arc<MobileIdentityRegistry>,
    source: MobileSlotSelection,
}

impl Fixture {
    fn new() -> Self {
        Self::with_keys(Arc::new(Credentials::default()))
    }

    fn with_keys(keys: Arc<dyn crate::MobileSlotKeys>) -> Self {
        let root = root();
        let registry = MobileIdentityRegistry::new(
            root.to_str().unwrap().into(),
            MobileSlotLayout::Android,
            keys,
        );
        let source = registry.create().unwrap();
        Self {
            root,
            registry,
            source,
        }
    }

    fn receiver(&self) -> Arc<MobileLinkReceiver> {
        MobileLinkReceiver::start(
            self.registry.clone(),
            self.source.slot.id.clone(),
            self.source.key.peer_id(),
            false,
        )
        .unwrap()
    }
}

#[test]
fn idle_native_receiver_holds_no_mutation_lease_and_cancellation_prevents_polling() {
    let fixture = Fixture::new();
    let receiver = fixture.receiver();
    let parsed = dyapp_client::link::Code::parse(&receiver.code(), dyapp_client::now()).unwrap();
    assert_eq!(parsed.expires_at, receiver.expires_at());
    let projected = receiver.qr_matrix().unwrap();
    let expected =
        crate::MobileQrMatrix::from(dyapp_client::qr::matrix(receiver.code().as_bytes()).unwrap());
    assert_eq!(projected, expected);
    assert_eq!(receiver.phase(), MobileLinkPhase::Waiting);
    assert!(receiver.result().unwrap().is_none());
    fixture.registry.create().unwrap();
    assert_eq!(receiver.cancel(), MobileLinkPhase::Cancelled);
    assert_eq!(receiver.poll().unwrap(), MobileLinkPhase::Cancelled);
    assert!(receiver.result().unwrap().is_none());
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
}

#[test]
fn native_poll_without_active_network_preserves_source_and_can_be_retried() {
    let fixture = Fixture::new();
    let receiver = fixture.receiver();
    assert!(receiver.poll().is_err());
    assert_eq!(receiver.phase(), MobileLinkPhase::Waiting);
    assert!(receiver.result().unwrap().is_none());
    assert_eq!(fixture.registry.list().unwrap().len(), 1);
    assert!(MobileLinkReceiver::start(
        fixture.registry.clone(),
        fixture.source.slot.id.clone(),
        "wrong owner".into(),
        false
    )
    .is_err());
    fixture.registry.create().unwrap();
    assert!(MobileLinkReceiver::start(
        fixture.registry,
        fixture.source.slot.id,
        fixture.source.key.peer_id(),
        false
    )
    .is_err());
}

#[test]
fn native_writer_returns_an_opaque_verified_identity_and_cloneable_receipt() {
    let fixture = Fixture::new();
    let identity = Identity::try_generate().unwrap();
    let expected = identity.peer_id();
    let mut received = ReceivedLink::new(
        &Registry::new(fixture.root, Layout::Android),
        fixture.source.slot.id,
        fixture.source.key.peer_id(),
        false,
        dyapp_client::link::Linked {
            identity,
            state: b"private state".to_vec(),
        },
        Identity::try_generate().unwrap(),
    );
    let selection = install(&mut received, fixture.registry.link_keys()).unwrap();
    assert_eq!(selection.key.peer_id(), expected);
    let cloned = selection.clone();
    assert_eq!(cloned.durability_detail, selection.durability_detail);
    assert!(Arc::ptr_eq(&selection.key, &cloned.key));
    assert_eq!(cloned.slot.id, selection.slot.id);
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
}

#[test]
fn failure_projection_keeps_exact_destination_and_original_detail() {
    use dyapp_client::identity_slots::LinkFailure;
    let failure = MobileLinkFailure::from(&LinkFailure::Destination {
        slot_id: "exact destination".into(),
        detail: "original error".into(),
    });
    assert_eq!(
        failure,
        MobileLinkFailure::Destination {
            slot_id: "exact destination".into(),
            detail: "original error".into(),
        }
    );
    assert_eq!(
        MobileLinkFailure::from(&LinkFailure::Unconfirmed("unknown".into())),
        MobileLinkFailure::Unconfirmed {
            detail: "unknown".into()
        }
    );
    assert_eq!(
        MobileLinkFailure::from(&LinkFailure::Admission("source".into())),
        MobileLinkFailure::Admission {
            detail: "source".into()
        }
    );
    assert_eq!(
        MobileLinkFailure::from(&LinkFailure::Reservation("reservation".into())),
        MobileLinkFailure::Reservation {
            detail: "reservation".into()
        }
    );
}
