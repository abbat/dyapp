//! A pair's chat state and texts over the selected identity folder's active network; see
//! docs/design/chats-and-matches.md.

use crate::mls::unwrap_state_key;
use crate::profile_draft::database;
use crate::{CoreError, IdentityKey, KeyStore};
use dyapp_client::chat::Event;
use dyapp_client::publish;
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use dyapp_client::Node;
use dyapp_identity::Identity;
use dyapp_messaging::mls::Device;
use dyapp_p2p_net::proto::Status;
use dyapp_profile::VerifiedProfile;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

mod actions;
mod contact_notes;
mod creation;
pub use contact_notes::ContactNoteError;
mod favorites;
mod forward;
pub use favorites::FavoriteError;
pub use forward::{ChatForwardError, ChatForwardSource};
mod like_submission;
mod service;
pub use like_submission::{ChatLikeAttempt, ChatLikeState};
mod media_cache;
mod notices;
mod original;
mod photo_settings;
pub use media_cache::ChatMediaCache;
pub use photo_settings::ChatPhotoDownload;
mod photo_view;
mod photos;
pub use photo_view::{ChatItem, ChatPhoto};
mod sticker_view;
pub use photos::chat_photo_hours_left;
pub use sticker_view::ChatSticker;
mod reads;
mod send_photo;
mod submission;
pub use submission::ChatSubmission;
mod view;
pub use actions::{ChatBlock, ChatRevocation};
pub use notices::ChatNotice;
pub use view::{ChatDelivery, ChatHistory, ChatLikes, ChatMessage, ChatRow};

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

/// `full` when the node's quota has no room for the photos, as on desktop; otherwise the text.
fn publish_failed(failure: publish::Failure) -> CoreError {
    match failure {
        publish::Failure::Node(dyapp_client::Error::Status(Status::Full)) => failed("full"),
        failure => failed(failure),
    }
}

/// `KeyPackages` each publish adds, so peers can invite this device.
pub(crate) const KEY_PACKAGES: usize = 4;

/// A new MLS state key encrypted by `store`, for the app to persist once per identity before
/// the first [`ChatSession::open`].
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if state-key generation or the platform keystore wrapping fails.
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
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
    /// A reply or a match opened the chat; only an ended match closes it.
    Open,
    /// A match ended; a new match reopens the chat.
    Closed,
}

impl From<Conversation> for ChatState {
    fn from(conversation: Conversation) -> Self {
        match conversation {
            Conversation::None => Self::None,
            Conversation::RequestSent(_) => Self::RequestSent,
            Conversation::RequestReceived(_) => Self::RequestReceived,
            Conversation::Open => Self::Open,
            Conversation::Closed => Self::Closed,
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
    // Drop after the session and all key owners, retaining file locks through completion.
    _operation: Option<dyapp_client::identity_slots::ChatOperation>,
}

#[uniffi::export]
impl ChatSession {
    /// Open the chats of `key`'s identity folder `dir`. The first open certifies a device with
    /// the mailbox key `device_key` and seals it in client.db under the state key `store`
    /// unwraps from `wrapped_state_key`. A lost key is refused, so it never resets MLS state.
    #[uniffi::constructor]
    ///
    /// # Errors
    ///
    /// Returns an error if the identity directory, unwrapped key, stored device state or device-key
    /// binding is invalid, or storage access or runtime initialization fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn open(
        dir: String,
        key: Arc<IdentityKey>,
        device_key: Arc<IdentityKey>,
        store: Arc<dyn KeyStore>,
        wrapped_state_key: Vec<u8>,
    ) -> Result<Arc<Self>, CoreError> {
        let dir = database(&dir)?.to_path_buf();
        let seal_key = unwrap_state_key(&store, wrapped_state_key)?;
        Self::from_keys(dir, key, device_key, seal_key, None)
    }
}

async fn refill_if_due(
    session: &mut Session,
    node: &mut Node,
    now: u64,
    seal_key: &[u8; 32],
) -> Result<(), CoreError> {
    // A failed refill stays due for the next call.
    if session.store().token_refill_due(now).map_err(failed)?
        && session
            .refill(std::slice::from_mut(node), seal_key)
            .await
            .is_ok()
    {
        session.store().token_refill_sent(now).map_err(failed)?;
    }
    Ok(())
}

fn likes_in(store: &Store, peer_id: &str) -> Result<ChatLikes, CoreError> {
    let likes = store.likes(peer_id).map_err(failed)?;
    Ok(ChatLikes {
        ours: likes.ours,
        matched: likes.matched(),
        passed: store.passed(peer_id).map_err(failed)?,
    })
}

/// This identity's published profile, signed again from its stored draft; `None` before the first
/// publish.
fn own_profile(store: &Store, identity: &Identity) -> Result<Option<VerifiedProfile>, CoreError> {
    let own = store.profile_draft().map_err(failed)?.unwrap_or_default();
    if own.version == 0 {
        return Ok(None);
    }
    dyapp_profile::verify(&own.sign(identity))
        .map(Some)
        .map_err(failed)
}

/// The own profile to send under: peers refuse a device it does not list.
fn listed_own(session: &Session, identity: &Identity) -> Result<VerifiedProfile, CoreError> {
    let certificate = session.certificate().map_err(failed)?;
    own_profile(session.store(), identity)?
        .filter(|own| own.profile.devices.contains(&certificate))
        .ok_or_else(|| CoreError::Failed("unpublished".into()))
}

/// Apply one mailbox fetch, then once more if that fetched profiles of owners it did not know,
/// such as the Welcome of a first text. Returns how many events changed a chat.
async fn apply_mailbox(
    session: &mut Session,
    node: &mut Node,
    own: Option<VerifiedProfile>,
    device_key: &Identity,
    seal_key: &[u8; 32],
) -> Result<u32, CoreError> {
    // shortcut: every call fetches the senders' profiles again, since the app opens a session
    // per call; keep them in the Store if mailbox polls start costing too many requests.
    let mut profiles: Vec<VerifiedProfile> = own.into_iter().collect();
    let mut changed = 0;
    for _ in 0..2 {
        let events = session
            .receive(std::slice::from_mut(node), device_key, &profiles, seal_key)
            .await
            .map_err(failed)?;
        let mut fetched = false;
        for event in events {
            match event {
                Event::UnknownOwner(peer) => {
                    fetched |= fetch_unknown_owner(node, &mut profiles, &peer).await;
                }
                Event::Gap(_) | Event::Refused(_) => {}
                _ => changed += 1,
            }
        }
        if !fetched {
            break;
        }
    }
    Ok(changed)
}

async fn fetch_unknown_owner(
    node: &mut Node,
    profiles: &mut Vec<VerifiedProfile>,
    peer: &str,
) -> bool {
    if profiles.iter().any(|known| known.peer_id == peer) {
        return false;
    }
    if let Ok(found) = node.get_profile(peer).await {
        profiles.push(found);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests;
