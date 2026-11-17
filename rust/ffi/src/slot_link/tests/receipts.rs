use super::*;
use crate::{IdentityKey, MobileSlotKeys, MobileSlotState, WrapError};
use dyapp_identity::network::{self, Settings};

fn deliver(fixture: &Fixture, receiver: &MobileLinkReceiver, owner: &Identity) -> wire::Mailbox {
    let code = dyapp_client::link::Code::parse(&receiver.code(), dyapp_client::now()).unwrap();
    let server = wire::Mailbox::new(&code, owner, b"authenticated private link state");
    network::save(
        &PathBuf::from(&fixture.source.slot.directory),
        Settings::Active,
        &server.network,
    )
    .unwrap();
    server
}

fn installed(receiver: &MobileLinkReceiver) -> MobileSlotSelection {
    let Some(MobileLinkReceipt::Installed { selection }) = receiver.result().unwrap() else {
        panic!("native receiver lost its committed installation receipt");
    };
    selection
}

#[test]
fn native_receipt_retains_actual_wire_installation_after_selection_changes_and_server_shutdown() {
    let fixture = Fixture::new();
    let receiver = fixture.receiver();
    let owner = Identity::try_generate().unwrap();
    let mut server = deliver(&fixture, &receiver, &owner);
    assert_eq!(receiver.poll().unwrap(), MobileLinkPhase::Completed);
    let first = installed(&receiver);
    assert_eq!(first.key.peer_id(), owner.peer_id());
    assert_eq!(first.slot.state, MobileSlotState::Ready);
    assert_ne!(first.slot.id, fixture.source.slot.id);
    let store = dyapp_client::store::Store::open_existing(
        &PathBuf::from(&first.slot.directory).join("client.db"),
        &owner,
    )
    .unwrap();
    assert_eq!(
        store.link_pending().unwrap().unwrap(),
        b"authenticated private link state"
    );
    drop(store);
    fixture.registry.create().unwrap();
    server.shutdown();
    assert_eq!(receiver.poll().unwrap(), MobileLinkPhase::Completed);
    assert_eq!(receiver.cancel(), MobileLinkPhase::Completed);
    let repeated = installed(&receiver);
    assert_eq!(repeated.slot.id, first.slot.id);
    assert_eq!(repeated.durability_detail, first.durability_detail);
    assert!(Arc::ptr_eq(&repeated.key, &first.key));
    assert_eq!(fixture.registry.list().unwrap().len(), 3);
}

struct DeniedDevice(Credentials);
impl MobileSlotKeys for DeniedDevice {
    fn contains(&self, id: String) -> Result<bool, WrapError> {
        self.0.contains(id)
    }
    fn open_wrapped(
        &self,
        id: String,
        wrapped: Vec<u8>,
    ) -> Result<Option<Arc<IdentityKey>>, WrapError> {
        self.0.open_wrapped(id, wrapped)
    }
    fn wrap(&self, id: String, key: Arc<IdentityKey>) -> Result<Vec<u8>, WrapError> {
        self.0.wrap(id, key)
    }
    fn wrap_existing(&self, _: String, _: Arc<IdentityKey>) -> Result<Vec<u8>, WrapError> {
        Err(WrapError::Failed {
            reason: "received device wrapping refused".into(),
        })
    }
    fn erase(&self, id: String) -> Result<(), WrapError> {
        self.0.erase(id)
    }
}

fn failure(receiver: &MobileLinkReceiver) -> MobileLinkFailure {
    let Some(MobileLinkReceipt::Failed { failure }) = receiver.result().unwrap() else {
        panic!("native receiver lost its original platform failure receipt");
    };
    failure
}

#[test]
fn native_receipt_retains_the_exact_failed_destination_without_retrying_the_platform_write() {
    let fixture = Fixture::with_keys(Arc::new(DeniedDevice(Credentials::default())));
    let receiver = fixture.receiver();
    let mut server = deliver(&fixture, &receiver, &Identity::try_generate().unwrap());
    assert_eq!(receiver.poll().unwrap(), MobileLinkPhase::Failed);
    let original = failure(&receiver);
    let MobileLinkFailure::Destination { slot_id, detail } = &original else {
        panic!("received-device failure lost its actual reserved destination");
    };
    assert!(detail.contains("received device wrapping refused"));
    assert!(fixture
        .registry
        .list()
        .unwrap()
        .iter()
        .any(|slot| slot.id == *slot_id && slot.state == MobileSlotState::Pending));
    server.shutdown();
    assert_eq!(receiver.poll().unwrap(), MobileLinkPhase::Failed);
    assert_eq!(receiver.cancel(), MobileLinkPhase::Failed);
    assert_eq!(failure(&receiver), original);
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
}
