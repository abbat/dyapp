//! A pair's chat state and texts over the selected identity folder's active network; see
//! docs/design/chats-and-matches.md.

use crate::mls::{unwrap_state_key, verified};
use crate::profile_draft::database;
use crate::profile_network::{
    active_network, connect, on_network_stack, publishable_draft, send_held, ProfilePublished,
};
use crate::{CoreError, IdentityKey, KeyStore, PublicProfileDraft};
use dyapp_client::publish;
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use dyapp_messaging::mls::Device;
use dyapp_profile::DeviceCertificate;
use prost::Message as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

/// KeyPackages each publish adds, so peers can invite this device.
const KEY_PACKAGES: usize = 4;

/// A new MLS state key encrypted by `store`, for the app to persist once per identity before
/// the first [`ChatSession::open`].
#[uniffi::export]
pub fn new_chat_state_key(store: Arc<dyn KeyStore>) -> Result<Vec<u8>, CoreError> {
    let key = Zeroizing::new(Device::state_key()?);
    Ok(store.wrap(key.to_vec())?)
}

/// Where a pair's chat stands; the screens map it as in the design's "State of a pair".
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatState {
    /// No text and no match yet.
    None,
    /// This profile's one pending text waits for a reply (`chat-first`).
    RequestSent,
    /// The peer's one pending text waits for a reply (`chat-request`).
    RequestReceived,
    /// A reply or a match opened the chat; it stays open.
    Open,
}

impl From<Conversation> for ChatState {
    fn from(conversation: Conversation) -> Self {
        match conversation {
            Conversation::None => Self::None,
            Conversation::RequestSent(_) => Self::RequestSent,
            Conversation::RequestReceived(_) => Self::RequestReceived,
            Conversation::Open => Self::Open,
        }
    }
}

/// One identity's chats, kept in its folder's client.db; calls block and run off the UI thread.
#[derive(uniffi::Object)]
pub struct ChatSession {
    dir: PathBuf,
    key: Arc<IdentityKey>,
    device_key: Arc<IdentityKey>,
    runtime: tokio::runtime::Runtime,
    seal_key: Zeroizing<[u8; 32]>,
    session: Mutex<Session>,
}

#[uniffi::export]
impl ChatSession {
    /// Open the chats of `key`'s identity folder `dir`. The first open certifies a device with
    /// the mailbox key `device_key` and seals it in client.db under the state key `store`
    /// unwraps from `wrapped_state_key`. A lost key is refused, so it never resets MLS state.
    #[uniffi::constructor]
    pub fn open(
        dir: String,
        key: Arc<IdentityKey>,
        device_key: Arc<IdentityKey>,
        store: Arc<dyn KeyStore>,
        wrapped_state_key: Vec<u8>,
    ) -> Result<Arc<Self>, CoreError> {
        let dir = database(&dir)?.to_path_buf();
        let seal_key = unwrap_state_key(&store, wrapped_state_key)?;
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
        }))
    }

    /// Publish the solved draft listing this device, check the signed readback, then publish
    /// KeyPackages so peers can invite the device. Errors as in `ProfilePublisher::publish`.
    pub fn publish_profile(&self, attested: bool) -> Result<ProfilePublished, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let mut draft = publishable_draft(session.store(), attested)?;
        publish::list_device(&mut draft, session.certificate().map_err(failed)?);
        let network = active_network(&self.dir)?;
        let published = on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let published =
                    publish::publish(session.store(), &mut node, &self.key.identity, &draft)
                        .await
                        .map_err(failed)?;
                // shortcut: a fixed batch per publish, as on desktop; peers can exhaust it until
                // the next publish, so refill it when the receive loop lands.
                session
                    .publish_key_packages(
                        &mut node,
                        &self.device_key.identity,
                        KEY_PACKAGES,
                        &self.seal_key,
                    )
                    .await
                    .map_err(failed)?;
                send_held(session.store(), &mut node, &network).await?;
                Ok(published)
            })
        })?;
        Ok(ProfilePublished {
            draft: PublicProfileDraft::from(&published),
            version: published.version,
        })
    }

    /// Where the chat with `peer_id` stands.
    pub fn state(&self, peer_id: String) -> Result<ChatState, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        Ok(session.conversation(&peer_id).map_err(failed)?.into())
    }

    /// Send `text` to the one other owner in `profiles` (encoded `SignedRecord`s, the newest held
    /// for this owner and the peer) and return its logical ID. The first text invites the peer;
    /// a text to a pending request is the reply that opens the chat. A failed send stays staged
    /// and is retried with the same bytes.
    pub fn send_text(&self, profiles: Vec<Vec<u8>>, text: String) -> Result<Vec<u8>, CoreError> {
        let profiles = verified(&profiles)?;
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                session
                    .send_text_to(
                        std::slice::from_mut(&mut node),
                        &profiles,
                        text.as_bytes(),
                        &self.seal_key,
                    )
                    .await
                    .map(|id| id.to_vec())
                    .map_err(failed)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WrapError;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// XOR with a fixed byte: stands in for the platform keystore.
    struct TestStore;

    impl KeyStore for TestStore {
        fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
        }

        fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            self.wrap(wrapped)
        }
    }

    #[test]
    fn the_first_open_stores_the_device_for_later_opens() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-chats-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.to_str().unwrap().to_owned();
        let key = IdentityKey::generate();
        let device_key = IdentityKey::generate();
        let store: Arc<dyn KeyStore> = Arc::new(TestStore);
        let state_key = new_chat_state_key(store.clone()).unwrap();
        let open = |device_key: &Arc<IdentityKey>, state_key: &Vec<u8>| {
            ChatSession::open(
                dir.clone(),
                key.clone(),
                device_key.clone(),
                store.clone(),
                state_key.clone(),
            )
        };
        let chats = open(&device_key, &state_key).unwrap();
        assert_eq!(chats.state("ab".repeat(32)).unwrap(), ChatState::None);
        assert_eq!(
            chats
                .send_text(Vec::new(), "hi".into())
                .unwrap_err()
                .to_string(),
            "notConnected"
        );
        // A publish refuses at the same gates as the plain publisher, before any network use.
        assert_eq!(
            chats.publish_profile(false).unwrap_err().to_string(),
            "blocked: notAttested"
        );
        drop(chats);

        // The stored snapshot opens again, under the same keys only.
        open(&device_key, &state_key).unwrap();
        let other_state = new_chat_state_key(store.clone()).unwrap();
        assert!(open(&device_key, &other_state).is_err());
        assert_eq!(
            open(&IdentityKey::generate(), &state_key)
                .err()
                .unwrap()
                .to_string(),
            "the device key changed"
        );
    }
}
