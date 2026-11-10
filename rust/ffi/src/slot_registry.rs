//! Thin common mobile bindings for the shared identity registry.

use crate::slot_keys::SlotKeys;
use crate::{CoreError, IdentityKey, MobileSlotKeys};
use dyapp_client::identity_slots as core;
use std::sync::Arc;

/// Existing wrapped-key layout to read; never a migration request.
#[derive(Clone, Copy, uniffi::Enum)]
pub enum MobileSlotLayout {
    /// Android Keystore layout.
    Android,
    /// iOS Keychain layout, including deletion markers.
    Ios,
}

/// Lifecycle read by the core registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileSlotState {
    /// Creation is incomplete.
    Pending,
    /// Key creation was verified.
    Ready,
    /// Confirmed removal is incomplete.
    Deleting,
    /// Metadata is damaged.
    Broken,
}

/// One core-inventoried identity location.
#[derive(Clone, uniffi::Record)]
pub struct MobileSlot {
    /// Canonical slot identifier or legacy-primary.
    pub id: String,
    /// Existing data directory.
    pub directory: String,
    /// Lifecycle classification.
    pub state: MobileSlotState,
}

impl From<core::Slot> for MobileSlot {
    fn from(slot: core::Slot) -> Self {
        let state = match slot.state {
            core::State::Pending => MobileSlotState::Pending,
            core::State::Ready => MobileSlotState::Ready,
            core::State::Deleting => MobileSlotState::Deleting,
            core::State::Broken => MobileSlotState::Broken,
        };
        Self {
            id: slot.id,
            directory: slot.dir.to_string_lossy().into_owned(),
            state,
        }
    }
}

/// A committed selection and its independent directory-persistence status.
#[derive(uniffi::Record)]
pub struct MobileSlotSelection {
    /// Slot whose key actually opened.
    pub slot: MobileSlot,
    /// Opaque verified identity key.
    pub key: Arc<IdentityKey>,
    /// None when synchronized, otherwise a diagnostic after visible commit.
    pub durability_detail: Option<String>,
}

pub(crate) fn selected(
    slot: core::Slot,
    identity: dyapp_identity::Identity,
    durability: core::SelectionDurability,
) -> MobileSlotSelection {
    let durability_detail = match durability {
        core::SelectionDurability::Synced => None,
        core::SelectionDurability::Unconfirmed { detail } => Some(detail),
    };
    MobileSlotSelection {
        slot: slot.into(),
        key: Arc::new(IdentityKey { identity }),
        durability_detail,
    }
}

/// Common Android/iOS registry; platform callbacks own only credentials.
#[derive(uniffi::Object)]
pub struct MobileIdentityRegistry {
    registry: core::Registry,
    keys: SlotKeys,
}

#[uniffi::export]
impl MobileIdentityRegistry {
    /// Bind existing mobile storage without creating an identity.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(root: String, layout: MobileSlotLayout, keys: Arc<dyn MobileSlotKeys>) -> Arc<Self> {
        let layout = match layout {
            MobileSlotLayout::Android => core::Layout::Android,
            MobileSlotLayout::Ios => core::Layout::Ios,
        };
        Arc::new(Self {
            registry: core::Registry::new(root.into(), layout),
            keys: SlotKeys(keys),
        })
    }

    /// Read the core inventory under its registry lock.
    ///
    /// # Errors
    /// Returns lock or inventory failures without treating them as fresh setup.
    pub fn list(&self) -> Result<Vec<MobileSlot>, CoreError> {
        self.registry
            .list()
            .map(|slots| slots.into_iter().map(Into::into).collect())
            .map_err(CoreError::Failed)
    }

    /// Resolve existing storage without generating an identity or changing selection.
    ///
    /// # Errors
    /// Returns lock and inventory failures; key failures remain explicit recovery states.
    pub fn resolve(&self) -> Result<crate::MobileSlotResolution, CoreError> {
        self.registry
            .resolve(&self.keys)
            .map(Into::into)
            .map_err(CoreError::Failed)
    }

    /// Admit a verified identity operation and exclude mutations until its lease drops.
    ///
    /// # Errors
    /// Returns lock, inventory, lifecycle, key access and Peer ID mismatch failures.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned strings across the native boundary"
    )]
    pub fn operation(
        &self,
        id: String,
        expected_peer: String,
    ) -> Result<Arc<crate::MobileIdentityOperation>, CoreError> {
        let operation = self
            .registry
            .operation(&id, &expected_peer, &self.keys)
            .map_err(CoreError::Failed)?;
        Ok(Arc::new(crate::MobileIdentityOperation { operation }))
    }

    /// Explicitly create and verify a wrapped-key identity.
    ///
    /// # Errors
    /// Returns pre-selection failures; committed durability is carried in the result.
    pub fn create(&self) -> Result<MobileSlotSelection, CoreError> {
        let (slot, identity, durability) = self
            .registry
            .create(core::Storage::Wrapped, &self.keys)
            .map_err(CoreError::Failed)?;
        Ok(selected(slot, identity, durability))
    }

    /// Select a listed ready slot only after its actual key opens.
    ///
    /// # Errors
    /// Returns pre-selection failures; committed durability is carried in the result.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned strings across the native boundary"
    )]
    pub fn select(&self, id: String) -> Result<MobileSlotSelection, CoreError> {
        let (slot, identity, durability) = self
            .registry
            .select(&id, &self.keys)
            .map_err(CoreError::Failed)?;
        Ok(selected(slot, *identity, durability))
    }
}
