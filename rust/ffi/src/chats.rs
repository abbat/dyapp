//! A pair's chat state and texts over the selected identity folder's active network; see
//! docs/design/chats-and-matches.md.

use crate::contact_names::fetch_profiles;
use crate::mls::unwrap_state_key;
use crate::profile_draft::database;
use crate::profile_network::{
    active_network, connect, on_network_stack, publishable_draft, send_held, ProfilePublished,
};
use crate::{CoreError, FoundProfile, IdentityKey, KeyStore, PublicProfileDraft};
use dyapp_client::chat::{Event, MAX_TEXT_BYTES};
use dyapp_client::interactions::Kind;
use dyapp_client::publish;
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use dyapp_client::Node;
use dyapp_identity::Identity;
use dyapp_messaging::mls::Device;
use dyapp_p2p_net::proto::Status;
use dyapp_profile::{DeviceCertificate, VerifiedProfile};
use prost::Message as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

mod actions;
mod notices;
mod photo_view;
mod photos;
pub use photo_view::ChatPhoto;
mod reads;
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
        let owner = self.key.identity.public_key();
        let binding = dyapp_client::owner_binding(&self.device_key.identity, &owner);
        session.list_self(&mut draft, binding).map_err(failed)?;
        let network = active_network(&self.dir)?;
        let published = on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let published =
                    publish::publish(session.store(), &mut node, &self.key.identity, &draft)
                        .await
                        .map_err(publish_failed)?;
                session
                    .store()
                    .note_device_listed(published.version)
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

    /// Send `text` to `peer_id`, whose verified profile comes from the active network, and return
    /// its logical ID. The first text invites the peer; a text to a pending request is the reply
    /// that opens the chat. A failed send stays staged and is retried with the same bytes. Errors
    /// are `ownPeer`, `invalidText`, `blocked`, `unpublished` (this device or the peer has no
    /// published profile), `notConnected` or a node or storage message.
    pub fn send_text(&self, peer_id: String, text: String) -> Result<Vec<u8>, CoreError> {
        if peer_id == self.key.identity.peer_id() {
            return Err(CoreError::Failed("ownPeer".into()));
        }
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(CoreError::Failed("invalidText".into()));
        }
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        if session.store().blocked(&peer_id).map_err(failed)? {
            return Err(CoreError::Failed("blocked".into()));
        }
        let own = listed_own(session, &self.key.identity)?;
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let peer = node.get_profile(&peer_id).await.map_err(failed)?;
                if peer.profile.deleted || peer.profile.version == 0 {
                    return Err(CoreError::Failed("unpublished".into()));
                }
                session
                    .send_text_to(
                        std::slice::from_mut(&mut node),
                        &[own, peer],
                        text.as_bytes(),
                        &self.seal_key,
                    )
                    .await
                    .map(|id| id.to_vec())
                    .map_err(failed)
            })
        })
    }

    /// This identity's chats, newest first, without blocked peers.
    pub fn chat_rows(&self) -> Result<Vec<ChatRow>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        let mut rows = Vec::new();
        for row in store.chat_rows().map_err(failed)? {
            if !store.blocked(&row.peer_id).map_err(failed)? {
                let muted = store.muted(&row.peer_id).map_err(failed)?;
                rows.push(ChatRow::new(row, muted));
            }
        }
        Ok(rows)
    }

    /// The current messages with `peer_id`; `blocked` for a blocked peer.
    pub fn chat_history(&self, peer_id: String) -> Result<ChatHistory, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        if store.blocked(&peer_id).map_err(failed)? {
            return Err(CoreError::Failed("blocked".into()));
        }
        let mut messages: Vec<ChatMessage> = store
            .current_chat_messages(&peer_id)
            .map_err(failed)?
            .into_iter()
            .map(Into::into)
            .collect();
        let own = self.key.identity.peer_id();
        reads::project_reads(store, &peer_id, &own, &mut messages)?;
        Ok(ChatHistory {
            state: store.conversation(&peer_id).map_err(failed)?.into(),
            messages,
            muted: store.muted(&peer_id).map_err(failed)?,
        })
    }

    /// The like state with `peer_id`.
    pub fn likes(&self, peer_id: String) -> Result<ChatLikes, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        likes_in(session.store(), &peer_id)
    }

    /// The Liked you peers: each likes this profile, is not liked back and is not blocked, in
    /// Peer ID order. Local: the screen fetches their profiles.
    pub fn liked_you(&self) -> Result<Vec<String>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().liked_by().map_err(failed)
    }

    /// The verified profiles of `peer_ids` from the active network, in that order; a peer no node
    /// returns is left out. No peers means no connection. Errors are `notConnected` or a node
    /// message.
    pub fn profiles(&self, peer_ids: Vec<String>) -> Result<Vec<FoundProfile>, CoreError> {
        let found = fetch_profiles(&self.runtime, &self.dir, peer_ids)?;
        Ok(found.iter().map(FoundProfile::from).collect())
    }

    /// Like `peer_id` (`liked`) or take the like back, as a signal in the pair group on the active
    /// network; the first like invites the peer. A failed send stays staged and is retried with
    /// the same bytes. Errors are `unpublished` (no published own profile lists this device),
    /// `notConnected` or a node or storage message.
    pub fn like(&self, peer_id: String, liked: bool) -> Result<ChatLikes, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let own = listed_own(session, &self.key.identity)?;
        let network = active_network(&self.dir)?;
        let kind = if liked { Kind::Like } else { Kind::Unlike };
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let peer = node.get_profile(&peer_id).await.map_err(failed)?;
                session
                    .interact_with(
                        std::slice::from_mut(&mut node),
                        &[own, peer],
                        kind,
                        &self.seal_key,
                    )
                    .await
                    .map_err(failed)?;
                likes_in(session.store(), &peer_id)
            })
        })
    }

    /// Resend the staged writes that are due, apply this device's mailbox and refill tokens when
    /// due. Returns how many events changed a chat. An envelope from an owner the node has no
    /// profile for waits in the mailbox for the next call. Errors are `notConnected` or a node
    /// or storage message.
    pub fn receive(&self) -> Result<u32, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        let own = own_profile(session.store(), &self.key.identity)?;
        let network = active_network(&self.dir)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(failed)?;
        let now_ms = i64::try_from(now.as_millis()).map_err(failed)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                session
                    .retry_due(std::slice::from_mut(&mut node), now_ms, &self.seal_key)
                    .await
                    .map_err(failed)?;
                let changed = apply_mailbox(
                    session,
                    &mut node,
                    own,
                    &self.device_key.identity,
                    &self.seal_key,
                )
                .await?;
                // A failed refill stays due for the next call.
                if session
                    .store()
                    .token_refill_due(now.as_secs())
                    .map_err(failed)?
                    && session
                        .refill(std::slice::from_mut(&mut node), &self.seal_key)
                        .await
                        .is_ok()
                {
                    session
                        .store()
                        .token_refill_sent(now.as_secs())
                        .map_err(failed)?;
                }
                // Best effort like the refill, as on the desktop: once the first publish made a
                // last-resort package it rotates or repeats, and the photo hold renews daily.
                if matches!(session.store().last_resort(), Ok(Some(_))) {
                    let _ = session
                        .keep_last_resort(
                            &mut node,
                            &self.device_key.identity,
                            now.as_secs(),
                            &self.seal_key,
                        )
                        .await;
                }
                let _ = dyapp_client::publish::refresh_photos(
                    session.store(),
                    &mut node,
                    &self.key.identity,
                    now.as_secs(),
                )
                .await;
                Ok(changed)
            })
        })
    }
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
                    if profiles.iter().any(|known| known.peer_id == peer) {
                        continue;
                    }
                    if let Ok(found) = node.get_profile(&peer).await {
                        profiles.push(found);
                        fetched = true;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WrapError;

    #[test]
    fn a_full_node_gets_its_own_publish_code() {
        let full = publish::Failure::Node(dyapp_client::Error::Status(Status::Full));
        assert_eq!(publish_failed(full).to_string(), "full");
        let missed = publish_failed(publish::Failure::Missed).to_string();
        assert_eq!(missed, "the profile changed on another device");
    }

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
        assert_eq!(chats.receive().unwrap_err().to_string(), "notConnected");
        assert!(chats.chat_rows().unwrap().is_empty());
        let peer = "ab".repeat(32);
        // Even Always must keep a stranger behind a tap, regardless of network reports.
        chats
            .session
            .lock()
            .unwrap()
            .store()
            .set_auto_download(dyapp_client::attachment::AutoDownload::Always)
            .unwrap();
        assert!(!chats.chat_photo_auto(peer.clone()).unwrap());
        chats
            .session
            .lock()
            .unwrap()
            .store()
            .set_auto_download(dyapp_client::attachment::AutoDownload::ByNetwork)
            .unwrap();
        assert_eq!(
            chats.like(peer.clone(), true).unwrap_err().to_string(),
            "unpublished"
        );
        let send = |to: &str, text: &str| {
            let refused = chats.send_text(to.to_owned(), text.to_owned());
            refused.unwrap_err().to_string()
        };
        assert_eq!(send(&key.peer_id(), "hi"), "ownPeer");
        assert_eq!(send(&peer, ""), "invalidText");
        assert_eq!(send(&peer, &"x".repeat(MAX_TEXT_BYTES + 1)), "invalidText");
        assert_eq!(send(&peer, "hi"), "unpublished");
        assert_eq!(send(&peer, &"x".repeat(MAX_TEXT_BYTES)), "unpublished");
        assert_eq!(
            chats.likes(peer.clone()).unwrap(),
            ChatLikes {
                ours: false,
                matched: false,
                passed: false
            }
        );
        assert!(chats.liked_you().unwrap().is_empty());
        assert!(chats.profiles(Vec::new()).unwrap().is_empty());
        assert_eq!(
            chats.profiles(vec![peer.clone()]).unwrap_err().to_string(),
            "notConnected"
        );
        assert!(chats.chat_photos(peer.clone()).unwrap().is_empty());
        let empty = chats.chat_history(peer.clone()).unwrap();
        assert_eq!((empty.state, empty.messages), (ChatState::None, Vec::new()));
        // A chat with no state cannot be cleared or read; only a live own text can be edited.
        let err = |result: Result<u32, CoreError>| result.unwrap_err().to_string();
        assert_eq!(err(chats.clear_chat(peer.clone())), "unknownPeer");
        assert_eq!(err(chats.mark_read(peer.clone())), "unknownPeer");
        assert_eq!(err(chats.send_reads(peer.clone())), "unknownPeer");
        let name = |to: String| {
            chats
                .set_contact_name(to, "Bob".into())
                .unwrap_err()
                .to_string()
        };
        assert_eq!(name(peer.clone()), "unknownPeer");
        assert_eq!(name(key.peer_id()), "ownPeer");
        // Receipts are off until turned on.
        assert!(!chats.read_receipts().unwrap());
        chats.set_read_receipts(true).unwrap();
        assert!(chats.read_receipts().unwrap());
        assert_eq!(err(chats.clear_chat(key.peer_id())), "ownPeer");
        let edit = |to: &str, id: Vec<u8>, text: &str| {
            let refused = chats.edit_text(to.to_owned(), vec![1], id, text.to_owned());
            refused.unwrap_err().to_string()
        };
        assert_eq!(edit(&peer, vec![2; 16], ""), "invalidText");
        assert_eq!(edit(&key.peer_id(), vec![2; 16], "hi"), "ownPeer");
        assert_eq!(edit(&peer, vec![2; 16], "hi"), "unknownMessage");
        assert_eq!(edit(&peer, vec![2; 3], "hi"), "unknownMessage");
        let delete = chats.delete_text(peer.clone(), vec![1], vec![2; 16]);
        assert_eq!(delete.unwrap_err().to_string(), "unknownMessage");
        let refused = |result: Result<ChatBlock, CoreError>| result.unwrap_err().to_string();
        assert_eq!(refused(chats.block(key.peer_id())), "invalidPeer");
        assert_eq!(refused(chats.block("ab".into())), "invalidPeer");
        assert_eq!(
            chats.unblock("xy".repeat(32)).unwrap_err().to_string(),
            "invalidPeer"
        );
        // With no own devices and no issued tokens, a block is local and needs no revocation.
        assert_eq!(
            chats.block(peer.clone()).unwrap(),
            ChatBlock {
                revocation: ChatRevocation::NotNeeded,
                detail: None
            }
        );
        assert_eq!(send(&peer, "hi"), "blocked");
        assert_eq!(
            chats.chat_photo_auto(peer.clone()).unwrap_err().to_string(),
            "blocked"
        );
        assert_eq!(err(chats.clear_chat(peer.clone())), "blocked");
        assert_eq!(edit(&peer, vec![2; 16], "hi"), "blocked");
        assert_eq!(
            chats.chat_history(peer.clone()).unwrap_err().to_string(),
            "blocked"
        );
        assert_eq!(
            chats.chat_photos(peer.clone()).unwrap_err().to_string(),
            "blocked"
        );
        assert_eq!(chats.blocked_peers().unwrap(), std::slice::from_ref(&peer));
        chats.unblock(peer.clone()).unwrap();
        assert!(chats.blocked_peers().unwrap().is_empty());
        assert!(!chats.chat_history(peer.clone()).unwrap().muted);
        // A mute changes only the flag, and like a block it needs a valid other peer.
        chats.set_muted(peer.clone(), true).unwrap();
        assert!(chats.chat_history(peer.clone()).unwrap().muted);
        chats.set_muted(peer.clone(), false).unwrap();
        assert!(!chats.chat_history(peer.clone()).unwrap().muted);
        assert_eq!(
            chats
                .set_muted(key.peer_id(), true)
                .unwrap_err()
                .to_string(),
            "invalidPeer"
        );
        // A pass is local, shows in the like state and the list, and is undone the same way.
        assert!(chats.set_passed(peer.clone(), true).unwrap().passed);
        assert_eq!(chats.passed_peers().unwrap(), std::slice::from_ref(&peer));
        assert!(!chats.set_passed(peer.clone(), false).unwrap().passed);
        assert!(chats.passed_peers().unwrap().is_empty());
        let pass_self = chats.set_passed(key.peer_id(), true);
        assert_eq!(pass_self.unwrap_err().to_string(), "invalidPeer");
        // Off keeps the chosen content for the next turn-on.
        assert_eq!(chats.notice().unwrap(), ChatNotice::Fixed);
        chats.set_notice(ChatNotice::Text).unwrap();
        chats.set_notice(ChatNotice::Off).unwrap();
        assert_eq!(chats.notice().unwrap(), ChatNotice::Off);
        assert_eq!(chats.notice_content().unwrap(), ChatNotice::Text);
        chats.set_notice(ChatNotice::Name).unwrap();
        assert_eq!(chats.notice().unwrap(), ChatNotice::Name);
        chats.block(peer.clone()).unwrap();
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
