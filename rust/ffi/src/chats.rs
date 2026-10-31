//! A pair's chat state and texts over the selected identity folder's active network; see
//! docs/design/chats-and-matches.md.

use crate::mls::{verified, MlsDevice};
use crate::profile_draft::database;
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::{CoreError, IdentityKey};
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
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
    runtime: tokio::runtime::Runtime,
    seal_key: Zeroizing<[u8; 32]>,
    session: Mutex<Session>,
}

#[uniffi::export]
impl ChatSession {
    /// Open the chats of `key`'s identity folder `dir`. The first open stores `device`'s state in
    /// client.db; from then on client.db holds the MLS state and `device` only lends its state key.
    #[uniffi::constructor]
    pub fn open(
        dir: String,
        key: Arc<IdentityKey>,
        device: Arc<MlsDevice>,
    ) -> Result<Arc<Self>, CoreError> {
        let dir = database(&dir)?.to_path_buf();
        let mut store = Store::open(&dir.join("client.db"), &key.identity).map_err(failed)?;
        if store.sealed_device().map_err(failed)?.is_none() {
            store.stage_outbound(&device.seal()?, &[]).map_err(failed)?;
        }
        let seal_key = device.state_key.clone();
        let session = Session::open(store, &seal_key).map_err(failed)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            dir,
            runtime,
            seal_key,
            session: Mutex::new(session),
        }))
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let device = MlsDevice::new(key.clone(), IdentityKey::generate(), 1).unwrap();
        let chats = ChatSession::open(dir.clone(), key.clone(), device.clone()).unwrap();
        assert_eq!(chats.state("ab".repeat(32)).unwrap(), ChatState::None);
        assert_eq!(
            chats
                .send_text(Vec::new(), "hi".into())
                .unwrap_err()
                .to_string(),
            "notConnected"
        );
        drop(chats);

        // The stored snapshot opens again, under the same state key only.
        ChatSession::open(dir.clone(), key.clone(), device).unwrap();
        let other = MlsDevice::new(key.clone(), IdentityKey::generate(), 1).unwrap();
        assert!(ChatSession::open(dir, key, other).is_err());
    }
}
