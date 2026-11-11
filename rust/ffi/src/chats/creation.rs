//! Session construction with optional core registry ownership.

use super::{failed, ChatSession};
use crate::{CoreError, IdentityKey};
use dyapp_client::{identity_slots::ChatOperation, session::Session, store::Store};
use dyapp_identity::Identity;
use dyapp_profile::DeviceCertificate;
use prost::Message as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

impl ChatSession {
    pub(crate) fn from_operation(operation: ChatOperation) -> Result<Arc<Self>, CoreError> {
        let dir = operation.slot().dir.clone();
        let key = Arc::new(IdentityKey {
            identity: Identity::from_secret(&operation.identity().secret()),
        });
        let device = Arc::new(IdentityKey {
            identity: Identity::from_secret(&operation.device().secret()),
        });
        let seal = Zeroizing::new(*operation.state_key());
        Self::from_keys(dir, key, device, seal, Some(operation))
    }

    pub(super) fn from_keys(
        dir: PathBuf,
        key: Arc<IdentityKey>,
        device_key: Arc<IdentityKey>,
        seal_key: Zeroizing<[u8; 32]>,
        operation: Option<ChatOperation>,
    ) -> Result<Arc<Self>, CoreError> {
        let database = Store::open(&dir.join("client.db"), &key.identity).map_err(failed)?;
        let session =
            Session::open_or_create(database, &key.identity, &device_key.identity, &seal_key)
                .map_err(failed)?;
        let certificate = session.certificate().map_err(failed)?;
        let certified = DeviceCertificate::decode(certificate.payload.as_slice())
            .map_err(failed)?
            .device_key;
        if certified != device_key.identity.public_key() {
            return Err(CoreError::Failed("the device key changed".into()));
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            dir,
            key,
            device_key,
            runtime,
            seal_key,
            session: Mutex::new(session),
            _operation: operation,
        }))
    }
}
