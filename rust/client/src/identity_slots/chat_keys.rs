//! Wrapped mobile chat-key bootstrap and operation ownership.

mod storage;

use super::{IdentityOperation, KeyReader, Layout, Registry, Slot, State};
use dyapp_identity::Identity;
use std::fs::File;
use zeroize::Zeroizing;

/// Purpose of a wrapped chat secret, not a platform alias.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatKeyKind {
    /// Existing owner identity, used to verify the wrapping capability before bootstrap.
    Identity,
    /// Per-device mailbox identity seed.
    Device,
    /// MLS state sealing key.
    State,
}

impl ChatKeyKind {
    fn file_name(self) -> &'static str {
        match self {
            Self::Identity => "identity.key",
            Self::Device => "device.key",
            Self::State => "chat-state.key",
        }
    }
}

/// Platform wrapping only; core owns bootstrap, persistence and verification.
pub trait ChatKeyStore {
    /// Wrap a fresh random secret using this slot's existing credential.
    ///
    /// # Errors
    /// Returns access/wrapping failures without creating a replacement credential.
    fn wrap(&self, slot: &Slot, kind: ChatKeyKind, secret: &[u8; 32]) -> Result<Vec<u8>, String>;

    /// Open an existing wrapped secret; never generate a replacement.
    ///
    /// # Errors
    /// Returns access/decoding failures.
    fn unwrap(
        &self,
        slot: &Slot,
        kind: ChatKeyKind,
        wrapped: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String>;
}

/// Verified chat secrets and locks retained through actual session completion.
///
/// Enter directly, without an existing identity operation: bootstrap needs the
/// exclusive registry lock before shared operation admission. Do not clone guards.
#[must_use = "dropping the chat operation releases its mutation and session protection"]
pub struct ChatOperation {
    device: Identity,
    state_key: Zeroizing<[u8; 32]>,
    _chat_lock: File,
    identity: IdentityOperation,
}

impl ChatOperation {
    /// Existing verified slot.
    #[must_use]
    pub fn slot(&self) -> &Slot {
        self.identity.slot()
    }

    /// Existing verified owner identity.
    #[must_use]
    pub fn identity(&self) -> &Identity {
        self.identity.identity()
    }

    /// Stable device identity, never regenerated for an initialized chat database.
    #[must_use]
    pub fn device(&self) -> &Identity {
        &self.device
    }

    /// Existing or durably bootstrapped MLS sealing key.
    #[must_use]
    pub fn state_key(&self) -> &[u8; 32] {
        &self.state_key
    }
}

impl Registry {
    /// Prepare wrapped chat keys, then admit a verified serialized chat operation.
    /// Cooperating removals wait for the returned operation's completion.
    ///
    /// # Errors
    /// Returns unsupported layout, key, database, identity, lock or persistence failures.
    pub fn chat_operation(
        &self,
        id: &str,
        expected_peer: &str,
        keys: &dyn KeyReader,
        wrapping: &dyn ChatKeyStore,
    ) -> Result<ChatOperation, String> {
        if self.layout == Layout::Desktop {
            return Err("Wrapped chat bootstrap requires a mobile layout".into());
        }
        {
            let _guard = self.lock()?;
            let slot = self.ready_chat_slot(id, expected_peer, keys)?;
            storage::prepare(&slot.0, &slot.1, wrapping)?;
        }
        let identity = self.operation(id, expected_peer, keys)?;
        let chat_lock = super::registry::open_lock_file(&identity.slot().dir.join("chat.lock"))?;
        chat_lock.lock().map_err(|error| error.to_string())?;
        let device = storage::required(identity.slot(), ChatKeyKind::Device, wrapping)?;
        let state_key = storage::required(identity.slot(), ChatKeyKind::State, wrapping)?;
        Ok(ChatOperation {
            device: Identity::from_secret(&device),
            state_key,
            _chat_lock: chat_lock,
            identity,
        })
    }

    fn ready_chat_slot(
        &self,
        id: &str,
        expected_peer: &str,
        keys: &dyn KeyReader,
    ) -> Result<(Slot, Box<Identity>), String> {
        let slot = super::list(&self.root, self.layout)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|slot| slot.id == id && slot.state == State::Ready)
            .ok_or("Identity slot is missing or not ready")?;
        let super::Resolution::Ready(slot, identity) = super::open_slot(slot, keys) else {
            return Err("Identity key is missing or unavailable".into());
        };
        if identity.peer_id() != expected_peer {
            return Err("The identity key changed".into());
        }
        Ok((slot, identity))
    }
}
