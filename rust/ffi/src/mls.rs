//! MLS end-to-end encryption over FFI: thin wrappers around `dyapp_messaging::mls`.
//!
//! Profiles cross as encoded `SignedRecord`s: the newest profile the app holds for every member's
//! owner. The app stores the wrapped state key once and a fresh [`MlsDevice::seal`] after every
//! call that changes state, before sending what that call returned.

use crate::{CoreError, IdentityKey, KeyStore};
use dyapp_identity::SignedRecord;
use dyapp_messaging::mls::{self, Device, Group, Received};
use dyapp_profile::VerifiedProfile;
use prost::Message as _;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

impl From<mls::Error> for CoreError {
    fn from(error: mls::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

pub(crate) fn verified(profiles: &[Vec<u8>]) -> Result<Vec<VerifiedProfile>, CoreError> {
    profiles
        .iter()
        .map(|bytes| {
            let record = SignedRecord::decode(bytes.as_slice())
                .map_err(|e| CoreError::Failed(e.to_string()))?;
            dyapp_profile::verify(&record).map_err(|e| CoreError::Failed(e.to_string()))
        })
        .collect()
}

fn poisoned<T>(_: T) -> CoreError {
    CoreError::Failed("MLS group lock poisoned".to_string())
}

/// One device's MLS keys and groups, with the random key that seals them.
#[derive(uniffi::Object)]
pub struct MlsDevice {
    device: Device,
    pub(crate) state_key: Zeroizing<[u8; 32]>,
}

#[uniffi::export]
impl MlsDevice {
    /// A device with a new MLS key, certified by `identity` with the device key `device_key`
    /// at `generation`, and a new state key.
    #[uniffi::constructor]
    pub fn new(
        identity: Arc<IdentityKey>,
        device_key: Arc<IdentityKey>,
        generation: u64,
    ) -> Result<Arc<Self>, CoreError> {
        Ok(Arc::new(Self {
            device: Device::new(
                &identity.identity,
                *device_key.identity.secret(),
                generation,
            )?,
            state_key: Zeroizing::new(Device::state_key()?),
        }))
    }

    /// The device that [`MlsDevice::seal`] wrote to `sealed`, under the state key `store`
    /// unwraps from `wrapped_key`.
    #[uniffi::constructor]
    pub fn open(
        store: Arc<dyn KeyStore>,
        wrapped_key: Vec<u8>,
        sealed: Vec<u8>,
    ) -> Result<Arc<Self>, CoreError> {
        let key = Zeroizing::new(store.unwrap(wrapped_key)?);
        let key: [u8; 32] = key
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Failed("unwrapped key is not 32 bytes".to_string()))?;
        let state_key = Zeroizing::new(key);
        Ok(Arc::new(Self {
            device: Device::open(&sealed, &state_key)?,
            state_key,
        }))
    }

    /// The state key encrypted by `store`, for the app to persist once.
    pub fn wrap_state_key(&self, store: Arc<dyn KeyStore>) -> Result<Vec<u8>, CoreError> {
        // ponytail: the FFI buffer that carries the key to the app is freed, not wiped.
        Ok(store.wrap(self.state_key.to_vec())?)
    }

    /// All MLS state encrypted under the state key.
    pub fn seal(&self) -> Result<Vec<u8>, CoreError> {
        Ok(self.device.seal(&self.state_key)?)
    }

    /// The encoded signed certificate for the owner's profile device list.
    pub fn certificate(&self) -> Vec<u8> {
        self.device.certificate().encode_to_vec()
    }

    /// A KeyPackage to publish; a single-use one serves one Welcome.
    pub fn key_package(&self, last_resort: bool) -> Result<Vec<u8>, CoreError> {
        Ok(self.device.key_package(last_resort)?)
    }

    /// A new group with the devices behind `key_packages`, and the Welcome to send them.
    pub fn create_group(
        &self,
        key_packages: Vec<Vec<u8>>,
        profiles: Vec<Vec<u8>>,
    ) -> Result<MlsNewGroup, CoreError> {
        let (group, welcome) = self
            .device
            .create_group(&key_packages, &verified(&profiles)?)?;
        Ok(MlsNewGroup {
            group: Arc::new(MlsGroup {
                group: Mutex::new(group),
            }),
            welcome,
        })
    }

    /// Joins the group that `welcome` invites this device to.
    pub fn join(
        &self,
        welcome: Vec<u8>,
        profiles: Vec<Vec<u8>>,
    ) -> Result<Arc<MlsGroup>, CoreError> {
        let group = self.device.join(&welcome, &verified(&profiles)?)?;
        Ok(Arc::new(MlsGroup {
            group: Mutex::new(group),
        }))
    }

    /// The group with `id` from this device's state, if any.
    pub fn load_group(&self, id: Vec<u8>) -> Result<Option<Arc<MlsGroup>>, CoreError> {
        Ok(Group::load(&self.device, &id)?.map(|group| {
            Arc::new(MlsGroup {
                group: Mutex::new(group),
            })
        }))
    }
}

/// A group [`MlsDevice::create_group`] started and its Welcome.
#[derive(uniffi::Record)]
pub struct MlsNewGroup {
    /// The new group.
    pub group: Arc<MlsGroup>,
    /// The Welcome to send to the added devices.
    pub welcome: Vec<u8>,
}

/// What an incoming group message carried.
#[derive(PartialEq, Eq, uniffi::Enum)]
pub enum MlsReceived {
    /// An application message from the member with `sender` as Peer ID.
    Message {
        /// Hex Peer ID of the sender's owner.
        sender: String,
        /// The decrypted bytes.
        plaintext: Vec<u8>,
    },
    /// A commit, now applied.
    Commit,
}

/// Leaves out the text and the sender.
impl std::fmt::Debug for MlsReceived {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message { .. } => f.debug_struct("Message").finish_non_exhaustive(),
            Self::Commit => f.write_str("Commit"),
        }
    }
}

/// One MLS group of an [`MlsDevice`]; every call takes that device.
#[derive(uniffi::Object)]
pub struct MlsGroup {
    group: Mutex<Group>,
}

#[uniffi::export]
impl MlsGroup {
    /// The group ID, for [`MlsDevice::load_group`].
    pub fn id(&self) -> Result<Vec<u8>, CoreError> {
        Ok(self.group.lock().map_err(poisoned)?.id().to_vec())
    }

    /// Hex Peer IDs of the members' owners.
    pub fn members(&self) -> Result<Vec<String>, CoreError> {
        Ok(self.group.lock().map_err(poisoned)?.members()?)
    }

    /// An application message for the group.
    pub fn encrypt(
        &self,
        device: Arc<MlsDevice>,
        plaintext: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError> {
        Ok(self
            .group
            .lock()
            .map_err(poisoned)?
            .encrypt(&device.device, &plaintext)?)
    }

    /// Decrypts a message or applies a commit from the group.
    pub fn process(
        &self,
        device: Arc<MlsDevice>,
        message: Vec<u8>,
        profiles: Vec<Vec<u8>>,
    ) -> Result<MlsReceived, CoreError> {
        let received = self.group.lock().map_err(poisoned)?.process(
            &device.device,
            &message,
            &verified(&profiles)?,
        )?;
        Ok(match received {
            Received::Message {
                sender, plaintext, ..
            } => MlsReceived::Message { sender, plaintext },
            Received::Commit => MlsReceived::Commit,
        })
    }

    /// The commit removing members their owner's profile no longer lists, if any.
    pub fn remove_revoked(
        &self,
        device: Arc<MlsDevice>,
        profiles: Vec<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        Ok(self
            .group
            .lock()
            .map_err(poisoned)?
            .remove_revoked(&device.device, &verified(&profiles)?)?)
    }

    /// `length` bytes from the epoch's exporter for `label` and `context`.
    pub fn export_secret(
        &self,
        device: Arc<MlsDevice>,
        label: String,
        context: Vec<u8>,
        length: u32,
    ) -> Result<Vec<u8>, CoreError> {
        let length = usize::try_from(length).map_err(|e| CoreError::Failed(e.to_string()))?;
        Ok(self.group.lock().map_err(poisoned)?.export_secret(
            &device.device,
            &label,
            &context,
            length,
        )?)
    }
}

#[cfg(test)]
mod tests;
