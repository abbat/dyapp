//! One received record and one installation outcome, retained until explicit disposal.

use super::{
    ChatKeyStore, KeyRemover, KeyWriter, LinkFailure, Registry, SelectionDurability, Slot,
};
use crate::link::Linked;
use dyapp_identity::Identity;
use zeroize::Zeroizing;

type Installation = Result<(Slot, Identity, SelectionDurability), LinkFailure>;

/// An owned received link, bound to its original registry and invitation source.
/// Holds no operation lease while idle. An outcome is a receipt, not current selection.
/// Repeating installation returns that same outcome; recovery needs a separate explicit action.
#[must_use = "retain the received link through installation or explicit recovery disposition"]
pub struct ReceivedLink {
    registry: Registry,
    from: String,
    expected_peer: String,
    wizard: bool,
    identity: Identity,
    state: Zeroizing<Vec<u8>>,
    device: Identity,
    outcome: Option<Installation>,
}

impl ReceivedLink {
    /// Retain the actual received record and device without reserving or writing anything.
    /// The receiver supplies the source captured when its invitation was admitted.
    pub fn new(
        registry: &Registry,
        from: String,
        expected_peer: String,
        wizard: bool,
        linked: Linked,
        device: Identity,
    ) -> Self {
        Self {
            registry: Registry::new(registry.root.clone(), registry.layout),
            from,
            expected_peer,
            wizard,
            identity: linked.identity,
            state: Zeroizing::new(linked.state),
            device,
            outcome: None,
        }
    }

    /// Attempt wrapped mobile installation once, retaining its exact success/failure receipt.
    /// Call only after receiver cancellation admission and release of shared identity operations.
    ///
    /// # Errors
    /// Returns the original typed failure on every later call, without reserving another slot.
    pub fn install<K: KeyWriter + KeyRemover + ChatKeyStore>(
        &mut self,
        keys: &K,
    ) -> Result<&(Slot, Identity, SelectionDurability), &LinkFailure> {
        if self.registry.layout == super::Layout::Desktop && self.outcome.is_none() {
            self.outcome = Some(Err(LinkFailure::Admission(
                "Wrapped linking requires a mobile layout".into(),
            )));
        }
        self.install_with(keys, &|slot, device| {
            super::chat_keys::persist_link_device(slot, device, keys)
        })
    }

    /// Install through a platform writer that stores and verifies the received device key.
    /// Only platform credential IO belongs in the callback; core owns the slot and outcome.
    /// Call after cancellation admission and release of shared identity operations.
    ///
    /// # Errors
    /// Retains the first failure without invoking another writer or reserving another slot.
    pub fn install_with<K: KeyWriter + KeyRemover>(
        &mut self,
        keys: &K,
        device: &dyn Fn(&Slot, &Identity) -> Result<(), String>,
    ) -> Result<&(Slot, Identity, SelectionDurability), &LinkFailure> {
        let first = self.outcome.is_none();
        // An unwinding platform callback must not turn the next poll into a fresh installation.
        let outcome = self.outcome.get_or_insert_with(|| {
            Err(LinkFailure::Unconfirmed(
                "Link installation returned no normal result".into(),
            ))
        });
        if first {
            *outcome = self.registry.link_selected_with(
                &self.from,
                &self.expected_peer,
                super::link::LinkRecord {
                    identity: &self.identity,
                    state: &self.state,
                },
                self.wizard,
                keys,
                &|slot| device(slot, &self.device),
            );
        }
        outcome.as_ref()
    }
}
