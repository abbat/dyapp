//! Likes and the Liked you list of the open identity on its active network, over the client
//! core's `Session`; see docs/design/chats-and-matches.md.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use dyapp_client::chat::Event;
use dyapp_client::interactions::Kind;
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use dyapp_profile::VerifiedProfile;

use crate::profile;
use crate::search::{self, Found, ProfileCache};

#[path = "chats/forward.rs"]
pub(crate) mod forward;
#[path = "chats/names.rs"]
pub(crate) mod names;
#[path = "chats/operations.rs"]
pub(crate) mod operations;
#[path = "chats/presence.rs"]
pub(crate) mod presence;
#[path = "chats/reads.rs"]
mod reads;
#[path = "chats/send_photo.rs"]
pub(crate) mod send_photo;
#[path = "chats/view.rs"]
mod view;
pub use view::{ChatHistoryView, ChatRowView, MessageView};

/// Held while an MLS session is open: two sessions would advance the same saved state, and the
/// later commit would drop what the other did.
#[derive(Default)]
pub struct Sessions(pub Arc<Mutex<()>>);

/// The like state with one peer as the page shows it.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Likes {
    /// This profile likes the peer: the page shows Liked.
    pub ours: bool,
    /// Both like each other.
    pub matched: bool,
    /// This profile passes on the peer on this device.
    pub passed: bool,
}

/// Network revocation is only confirmed by the node reached in this attempt.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Revocation {
    NotNeeded,
    Pending,
    Confirmed,
}

/// Local block always remains effective when network revocation is pending.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockResult {
    pub revocation: Revocation,
    pub detail: Option<String>,
}

impl BlockResult {
    fn pending(detail: impl ToString) -> Self {
        Self {
            revocation: Revocation::Pending,
            detail: Some(detail.to_string()),
        }
    }
}

pub(crate) fn open(dir: &Path, identity: &Identity) -> Result<Store, String> {
    Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())
}

pub fn chat_rows(dir: &Path, identity: &Identity) -> Result<Vec<ChatRowView>, String> {
    let store = open(dir, identity)?;
    let now = presence::now_ms()?;
    let mut rows = Vec::new();
    for row in store.chat_rows().map_err(|e| e.to_string())? {
        if store.blocked(&row.peer_id).map_err(|e| e.to_string())? {
            continue;
        }
        rows.push(ChatRowView {
            active_recently: store
                .active_recently(&row.peer_id, now)
                .map_err(|e| e.to_string())?,
            peer_id: row.peer_id,
            state: row.state.into(),
            last_message: view::last_item(row.last_message, row.last_attachment),
            unread_count: row.unread_count,
        });
    }
    Ok(rows)
}

pub fn chat_history(
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
) -> Result<ChatHistoryView, String> {
    let store = open(dir, identity)?;
    if store.blocked(peer_id).map_err(|e| e.to_string())? {
        return Err("blocked".into());
    }
    let mut messages: Vec<MessageView> = store
        .current_chat_items(peer_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(Into::into)
        .collect();
    reads::project_reads(&store, peer_id, &identity.peer_id(), &mut messages)?;
    view::mark_photos(&mut messages, presence::now_ms()?, dir);
    let state = store
        .conversation(peer_id)
        .map_err(|e| e.to_string())?
        .into();
    let active_recently = store
        .active_recently(peer_id, presence::now_ms()?)
        .map_err(|e| e.to_string())?;
    Ok(ChatHistoryView {
        state,
        messages,
        active_recently,
    })
}

pub(crate) fn open_visible_chat(dir: &Path, me: &Identity, peer_id: &str) -> Result<Store, String> {
    if peer_id == me.peer_id() {
        return Err("ownPeer".into());
    }
    let store = open(dir, me)?;
    if store.blocked(peer_id).map_err(|e| e.to_string())? {
        return Err("blocked".into());
    }
    if store.conversation(peer_id).map_err(|e| e.to_string())? == Conversation::None {
        return Err("unknownPeer".into());
    }
    Ok(store)
}

/// Clear this peer's entire device-local history while retaining its conversation state.
pub fn clear_chat(dir: &Path, identity: &Identity, peer_id: &str) -> Result<usize, String> {
    let store = open_visible_chat(dir, identity, peer_id)?;
    let removed = store.clear_chat_pair(peer_id).map_err(|e| e.to_string())?;
    store.sweep_originals(dir).map_err(|e| e.to_string())?;
    Ok(removed)
}

/// Mark current incoming texts in every group of a visible pair read on this device only.
pub fn mark_chat_read(dir: &Path, identity: &Identity, peer_id: &str) -> Result<usize, String> {
    open_visible_chat(dir, identity, peer_id)?
        .mark_chat_read(peer_id)
        .map_err(|e| e.to_string())
}

pub(crate) fn accept_peer(
    cache: &Mutex<ProfileCache>,
    epoch: u64,
    peer_id: &str,
    fetched: VerifiedProfile,
) -> Result<VerifiedProfile, String> {
    if fetched.peer_id != peer_id {
        return Err("unknownPeer".into());
    }
    let mut cache = cache.lock().map_err(|e| e.to_string())?;
    if !cache.current(epoch) {
        return Err("notConnected".into());
    }
    if !cache.accept_if_current(epoch, fetched) {
        return Err("staleProfile".into());
    }
    cache
        .snapshot()
        .1
        .get(peer_id)
        .cloned()
        .ok_or("unknownPeer".into())
}

/// Send through the active network after refreshing the peer's signed profile. The Session
/// stages the text and any first Welcome before network delivery; a failed send stays retryable.
pub async fn send_text(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    peer_id: &str,
    text: &str,
) -> Result<[u8; 16], String> {
    if peer_id == identity.peer_id() {
        return Err("ownPeer".into());
    }
    if text.is_empty() || text.len() > dyapp_client::chat::MAX_TEXT_BYTES {
        return Err("invalidText".into());
    }
    let store = open(dir, identity)?;
    if store.blocked(peer_id).map_err(|e| e.to_string())? {
        return Err("blocked".into());
    }
    let own = profile::stored(&store)?;
    if own.version == 0 {
        return Err("unpublished".into());
    }
    let own = dyapp_profile::verify(&own.sign(identity)).map_err(|e| e.to_string())?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let epoch = cache.lock().map_err(|e| e.to_string())?.epoch();
    let mut node = profile::connect(&active).await?;
    let fetched = node.get_profile(peer_id).await.map_err(|e| e.to_string())?;
    let peer = accept_peer(cache, epoch, peer_id, fetched)?;
    if peer.profile.deleted || peer.profile.version == 0 {
        return Err("unpublished".into());
    }
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    if !own
        .profile
        .devices
        .contains(&session.certificate().map_err(|e| e.to_string())?)
    {
        return Err("unpublished".into());
    }
    session
        .send_text_to(
            std::slice::from_mut(&mut node),
            &[own, peer],
            text.as_bytes(),
            &seal_key,
        )
        .await
        .map_err(send_photo::code)
}

fn likes_in(store: &Store, peer_id: &str) -> Result<Likes, String> {
    let likes = store.likes(peer_id).map_err(|e| e.to_string())?;
    Ok(Likes {
        ours: likes.ours,
        matched: likes.matched(),
        passed: store.passed(peer_id).map_err(|e| e.to_string())?,
    })
}

/// The like state with `peer_id`.
pub fn likes(dir: &Path, identity: &Identity, peer_id: &str) -> Result<Likes, String> {
    likes_in(&open(dir, identity)?, peer_id)
}

/// Passes on `peer_id` or undoes that, on this device only; no signal goes out.
pub fn pass(dir: &Path, identity: &Identity, peer_id: &str, passed: bool) -> Result<Likes, String> {
    let mut store = open(dir, identity)?;
    store
        .pass(peer_id, passed, presence::now_ms()?)
        .map_err(|e| e.to_string())?;
    likes_in(&store, peer_id)
}

/// Persist a local block before trying to revoke this profile's issued write tokens. A retry
/// sends the same live keys again; a failed node call never undoes the local block.
pub async fn block(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
) -> Result<BlockResult, String> {
    search::block(dir, identity, peer_id)?;
    let store = open(dir, identity)?;
    let today = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs()
        / 86_400;
    let keys = match store.revoke_issued(peer_id, today as u32) {
        Ok(keys) => keys,
        Err(error) => return Ok(BlockResult::pending(error)),
    };
    if keys.is_empty() {
        return Ok(BlockResult {
            revocation: Revocation::NotNeeded,
            detail: None,
        });
    }
    let active = match network::load(dir, Settings::Active) {
        Ok(Some(active)) => active,
        Ok(None) => return Ok(BlockResult::pending("notConnected")),
        Err(error) => return Ok(BlockResult::pending(error)),
    };
    let mut node = match profile::connect(&active).await {
        Ok(node) => node,
        Err(error) => return Ok(BlockResult::pending(error)),
    };
    let device = match crate::device_key(root, dir, identity) {
        Ok(device) => device,
        Err(error) => return Ok(BlockResult::pending(error)),
    };
    let seal_key = match crate::sealing_key(root, dir, &store) {
        Ok(key) => key,
        Err(error) => return Ok(BlockResult::pending(error)),
    };
    let mut session = match Session::open_or_create(store, identity, &device, &seal_key) {
        Ok(session) => session,
        Err(error) => return Ok(BlockResult::pending(error)),
    };
    match session
        .block(std::slice::from_mut(&mut node), &device, peer_id)
        .await
    {
        Ok(()) => Ok(BlockResult {
            revocation: Revocation::Confirmed,
            detail: None,
        }),
        Err(error) => Ok(BlockResult::pending(error)),
    }
}

/// Likes `peer_id` (`liked`) or takes the like back, as a signal in the pair group on the active
/// network; the first like invites the peer's devices. Only a peer a search or Liked you list of
/// this run returned can be liked. A send that fails stays staged with its bytes. Errors are
/// `unknownPeer`, `unpublished` (no published own profile lists this device), `notConnected` or
/// a node or storage message.
pub async fn like(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    peer_id: &str,
    liked: bool,
) -> Result<Likes, String> {
    let (epoch, profiles) = cache.lock().map_err(|e| e.to_string())?.snapshot();
    let peer = profiles.get(peer_id).cloned().ok_or("unknownPeer")?;
    let store = open(dir, identity)?;
    let own = profile::stored(&store)?;
    if own.version == 0 {
        return Err("unpublished".into());
    }
    let own = dyapp_profile::verify(&own.sign(identity)).map_err(|e| e.to_string())?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    if !cache.lock().map_err(|e| e.to_string())?.current(epoch) {
        return Err("notConnected".into());
    }
    let mut node = profile::connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    let kind = if liked { Kind::Like } else { Kind::Unlike };
    session
        .interact_with(
            std::slice::from_mut(&mut node),
            &[own, peer],
            kind,
            &seal_key,
        )
        .await
        .map_err(|e| e.to_string())?;
    likes_in(session.store(), peer_id)
}

/// The Liked you peers (`Store::liked_by`) with their profiles verified from the active network,
/// cached so they can be liked or blocked; a peer whose profile no node returns is left out. With
/// no such peer it does not connect. Errors are `notConnected` or a storage message.
pub async fn liked_you(
    cache: &Mutex<ProfileCache>,
    dir: &Path,
    identity: &Identity,
) -> Result<Vec<Found>, String> {
    let peers = open(dir, identity)?.liked_by().map_err(|e| e.to_string())?;
    if peers.is_empty() {
        return Ok(Vec::new());
    }
    let epoch = cache.lock().map_err(|e| e.to_string())?.epoch();
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    if !cache.lock().map_err(|e| e.to_string())?.current(epoch) {
        return Err("notConnected".into());
    }
    let mut node = profile::connect(&active).await?;
    let mut found = Vec::new();
    for peer_id in peers {
        let Ok(verified) = node.get_profile(&peer_id).await else {
            continue;
        };
        found.push(Found::from(&verified));
        // An older, equal or previous-network result is not cached.
        cache
            .lock()
            .map_err(|e| e.to_string())?
            .accept_if_current(epoch, verified);
    }
    Ok(found)
}

/// Mailbox pages one `receive` call reads; the saved cursor lets the next call go on from there.
// ponytail: the next call waits for the poll, schedule it at once if a long backlog feels slow.
const RECEIVE_PAGES: usize = 8;

/// Fetches this device's mailbox on the active network and applies it page by page with
/// `Session::receive_page`, knowing the cached profiles and the published own one, until the
/// mailbox ends, `RECEIVE_PAGES` is reached or `stop` reports an identity transition. An envelope
/// from an owner not cached, such as the Welcome of a first like, gets that owner's profile from
/// the node and is applied on a pass from the head after the last page; one still unknown waits
/// in the mailbox without hiding the pages behind it. Returns how many events changed a chat.
/// Errors are `notConnected` or a node or storage message; earlier pages stay committed.
pub async fn receive(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    stop: impl Fn() -> bool,
) -> Result<usize, String> {
    let store = open(dir, identity)?;
    let own = profile::stored(&store)?;
    let own = match own.version {
        0 => None,
        _ => Some(dyapp_profile::verify(&own.sign(identity)).map_err(|e| e.to_string())?),
    };
    let (epoch, mut cached) = cache.lock().map_err(|e| e.to_string())?.snapshot();
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    if !cache.lock().map_err(|e| e.to_string())?.current(epoch) {
        return Err("notConnected".into());
    }
    let mut node = profile::connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    retry_outbox(&mut session, &mut node, &seal_key).await?;
    let mut changed = 0;
    for _ in 0..RECEIVE_PAGES {
        if stop() {
            break;
        }
        let mut from_head = false;
        let mut profiles = cached.all().to_vec();
        profiles.extend(own.clone());
        let (events, more) = session
            .receive_page(
                std::slice::from_mut(&mut node),
                device_key,
                &profiles,
                &seal_key,
            )
            .await
            .map_err(|e| e.to_string())?;
        for event in events {
            match event {
                Event::UnknownOwner(peer) => {
                    if let Ok(verified) = node.get_profile(&peer).await {
                        cache
                            .lock()
                            .map_err(|e| e.to_string())?
                            .accept_if_current(epoch, verified.clone());
                        from_head |= cached.accept(verified).is_ok();
                    }
                }
                Event::Gap(_) | Event::Refused(_) => {}
                _ => changed += 1,
            }
        }
        if !session.next_page(more, from_head).map_err(|e| e.to_string())? {
            break;
        }
    }
    refill_if_due(&mut session, &mut node, &seal_key).await?;
    // Rotates or repeats the package once the first profile publish made one; a failure stays due.
    if matches!(session.store().last_resort(), Ok(Some(_))) {
        let _ = session
            .keep_last_resort(&mut node, device_key, dyapp_client::now(), &seal_key)
            .await;
    }
    // Best effort like the package above; it tries again the next day.
    let now = dyapp_client::now();
    let _ = dyapp_client::publish::refresh_photos(session.store(), &mut node, identity, now).await;
    Ok(changed)
}

// A catch-up pass resumes exact staged writes after a failed send or restart. Session owns
// outbox backoff; this path never drafts another text.
async fn retry_outbox(
    session: &mut Session,
    node: &mut dyapp_client::Node,
    seal_key: &[u8; 32],
) -> Result<(), String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let now_ms = i64::try_from(now_ms).map_err(|e| e.to_string())?;
    session
        .retry_due(std::slice::from_mut(node), now_ms, seal_key)
        .await
        .map_err(|e| e.to_string())
}

async fn refill_if_due(
    session: &mut Session,
    node: &mut dyapp_client::Node,
    seal_key: &[u8; 32],
) -> Result<(), String> {
    // A catch-up pass runs every minute. A failed refill stays due for the next pass.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    if session
        .store()
        .token_refill_due(now)
        .map_err(|e| e.to_string())?
        && session
            .refill(std::slice::from_mut(node), seal_key)
            .await
            .is_ok()
    {
        session
            .store()
            .token_refill_sent(now)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The page's commands for likes and the mailbox; each one that opens an MLS session holds
/// `Sessions` on its blocking thread.
pub mod commands {
    use std::sync::Arc;
    use std::time::Duration;

    use tauri::Manager;

    use crate::search::{Found, SearchState};

    // ponytail: one fixed cadence, also the bound for reconnect, resume and due retries; add
    // wake sources if a minute of delivery delay becomes the complaint.
    const CATCH_UP_PERIOD: Duration = Duration::from_secs(60);

    /// Locally blocks a Peer ID and reports this attempt's network token revocation.
    #[tauri::command]
    pub(crate) async fn block_peer(
        app: tauri::AppHandle,
        state: tauri::State<'_, crate::IdentityState>,
        sessions: tauri::State<'_, super::Sessions>,
        peer_id: String,
    ) -> Result<super::BlockResult, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active().clone();
        let root = crate::config_dir(&app)?;
        let sessions = Arc::clone(&sessions.0);
        tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            let _session = sessions.lock().map_err(|error| error.to_string())?;
            tauri::async_runtime::block_on(super::block(&root, &dir, &identity, &peer_id))
        })
        .await
        .map_err(|error| error.to_string())?
    }

    /// Current Chats rows for the active identity, including empty conversations.
    #[tauri::command]
    pub(crate) fn chat_rows(
        state: tauri::State<'_, crate::IdentityState>,
    ) -> Result<Vec<super::ChatRowView>, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::chat_rows(dir, identity)
    }

    /// Ordered current messages for one peer of the active identity.
    #[tauri::command]
    pub(crate) fn chat_history(
        state: tauri::State<'_, crate::IdentityState>,
        peer_id: String,
    ) -> Result<super::ChatHistoryView, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::chat_history(dir, identity, &peer_id)
    }

    /// Clear this device's unread flags after a visible conversation is opened.
    #[tauri::command]
    pub(crate) async fn mark_chat_read(
        state: tauri::State<'_, crate::IdentityState>,
        sessions: tauri::State<'_, super::Sessions>,
        peer_id: String,
    ) -> Result<usize, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active().clone();
        let sessions = Arc::clone(&sessions.0);
        tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            let _session = sessions.lock().map_err(|e| e.to_string())?;
            super::mark_chat_read(&dir, &identity, &peer_id)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Clear all local MLS groups for this peer while keeping the conversation row.
    #[tauri::command]
    pub(crate) async fn clear_chat(
        state: tauri::State<'_, crate::IdentityState>,
        sessions: tauri::State<'_, super::Sessions>,
        peer_id: String,
    ) -> Result<usize, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active().clone();
        let sessions = Arc::clone(&sessions.0);
        tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            let _session = sessions.lock().map_err(|e| e.to_string())?;
            super::clear_chat(&dir, &identity, &peer_id)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Send first, reply or later text through the active identity's MLS Session.
    #[tauri::command]
    pub(crate) async fn send_chat_text(
        app: tauri::AppHandle,
        state: tauri::State<'_, crate::IdentityState>,
        searching: tauri::State<'_, SearchState>,
        sessions: tauri::State<'_, super::Sessions>,
        peer_id: String,
        text: String,
    ) -> Result<[u8; 16], String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active().clone();
        let root = crate::config_dir(&app)?;
        let cache = Arc::clone(&searching.0);
        let sessions = Arc::clone(&sessions.0);
        tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            let _session = sessions.lock().map_err(|e| e.to_string())?;
            let device = crate::device_key(&root, &dir, &identity)?;
            tauri::async_runtime::block_on(super::send_text(
                &cache, &root, &dir, &identity, &device, &peer_id, &text,
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Send the read receipts the shown chat owes; see `reads`.
    #[tauri::command]
    pub(crate) async fn send_read_receipts(
        app: tauri::AppHandle,
        state: tauri::State<'_, crate::IdentityState>,
        sessions: tauri::State<'_, super::Sessions>,
        peer_id: String,
    ) -> Result<usize, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active().clone();
        let root = crate::config_dir(&app)?;
        let sessions = Arc::clone(&sessions.0);
        tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            let _session = sessions.lock().map_err(|e| e.to_string())?;
            let device = crate::device_key(&root, &dir, &identity)?;
            tauri::async_runtime::block_on(super::reads::send_read_receipts(
                &root, &dir, &identity, &device, &peer_id,
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// The like state with a peer; see `likes`.
    #[tauri::command]
    pub(crate) fn peer_likes(
        state: tauri::State<'_, crate::IdentityState>,
        peer_id: String,
    ) -> Result<super::Likes, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::likes(dir, identity, &peer_id)
    }

    /// Likes a found peer or takes the like back; see `like`.
    #[tauri::command]
    pub(crate) async fn like_peer(
        app: tauri::AppHandle,
        state: tauri::State<'_, crate::IdentityState>,
        searching: tauri::State<'_, SearchState>,
        sessions: tauri::State<'_, super::Sessions>,
        peer_id: String,
        liked: bool,
    ) -> Result<super::Likes, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active().clone();
        let root = crate::config_dir(&app)?;
        let cache = Arc::clone(&searching.0);
        let sessions = Arc::clone(&sessions.0);
        // The client store is not `Sync`, as in `crate::publish_profile`.
        tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            let _session = sessions.lock().map_err(|e| e.to_string())?;
            let device = crate::device_key(&root, &dir, &identity)?;
            tauri::async_runtime::block_on(super::like(
                &cache, &root, &dir, &identity, &device, &peer_id, liked,
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// The peers whose like is unanswered; see `liked_you`.
    #[tauri::command]
    pub(crate) async fn liked_you(
        state: tauri::State<'_, crate::IdentityState>,
        searching: tauri::State<'_, SearchState>,
    ) -> Result<Vec<Found>, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active().clone();
        let cache = Arc::clone(&searching.0);
        // The client store is not `Sync`, as in `crate::publish_profile`.
        tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            tauri::async_runtime::block_on(super::liked_you(&cache, &dir, &identity))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    // One pass for the active slot; see `receive`. The lease and `Sessions` lock serialize it
    // with every other pass and chat command, and a switch waits for it to return.
    fn catch_up(app: &tauri::AppHandle) -> Result<usize, String> {
        let lease = app.state::<crate::IdentityState>().lease()?;
        let (identity, dir) = lease.active().clone();
        let root = crate::config_dir(app)?;
        let cache = Arc::clone(&app.state::<SearchState>().0);
        let sessions = Arc::clone(&app.state::<super::Sessions>().0);
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        let device = crate::device_key(&root, &dir, &identity)?;
        let stop = || lease.transitioning().unwrap_or(true);
        crate::notify::around_pass(app, &dir, &identity, || {
            tauri::async_runtime::block_on(super::receive(
                &cache, &root, &dir, &identity, &device, stop,
            ))
        })
    }

    /// Runs passes from the runtime, because a hidden page's timers can be suspended, and tells
    /// the page to re-read its own identity's rows; the call carries only a count.
    pub(crate) fn watch_mail(app: tauri::AppHandle) {
        std::thread::spawn(move || loop {
            std::thread::sleep(CATCH_UP_PERIOD);
            let changed = catch_up(&app).map_or("null".to_string(), |count| count.to_string());
            if let Some(page) = app.get_webview_window("main") {
                let _ = page.eval(format!("window.mailUpdated?.({changed})"));
            }
        });
    }

    /// Runs a pass now, for a page that needs fresh mail.
    #[tauri::command]
    pub(crate) async fn receive_mail(app: tauri::AppHandle) -> Result<usize, String> {
        // The client store is not `Sync`, as in `crate::publish_profile`.
        tauri::async_runtime::spawn_blocking(move || catch_up(&app))
            .await
            .map_err(|e| e.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(super) fn temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-chats-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn block_without_issued_tokens_needs_no_network() {
        let (dir, owner, peer) = (
            temp_dir(),
            Identity::generate(),
            Identity::generate().peer_id(),
        );
        assert_eq!(
            block(&dir, &dir, &owner, &peer).await,
            Ok(BlockResult {
                revocation: Revocation::NotNeeded,
                detail: None,
            })
        );
        assert!(open(&dir, &owner).unwrap().blocked(&peer).unwrap());
    }

    #[tokio::test]
    async fn offline_revocation_remains_retryable_without_undoing_the_block() {
        let (dir, owner, peer) = (
            temp_dir(),
            Identity::generate(),
            Identity::generate().peer_id(),
        );
        let store = open(&dir, &owner).unwrap();
        let today = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 86_400;
        let key = [7; 32];
        store
            .record_issued(&peer, &[1; 32], &key, today as u32)
            .unwrap();
        for _ in 0..2 {
            let result = block(&dir, &dir, &owner, &peer).await.unwrap();
            assert_eq!(result.revocation, Revocation::Pending);
            assert_eq!(result.detail.as_deref(), Some("notConnected"));
            let store = open(&dir, &owner).unwrap();
            assert!(store.blocked(&peer).unwrap());
            assert!(store.issued_revoked(&key).unwrap());
            assert_eq!(store.revoke_issued(&peer, today as u32).unwrap(), [key]);
        }
    }

    #[tokio::test]
    async fn only_a_returned_peer_can_be_liked_and_no_like_needs_no_network() {
        let (dir, alice, cache) = (temp_dir(), Identity::generate(), Mutex::default());
        let stranger = Identity::generate().peer_id().to_string();
        let device = Identity::generate();
        assert_eq!(
            like(&cache, &dir, &dir, &alice, &device, &stranger, true).await,
            Err("unknownPeer".into())
        );
        assert_eq!(liked_you(&cache, &dir, &alice).await, Ok(Vec::new()));
        assert_eq!(
            receive(&cache, &dir, &dir, &alice, &device, || false).await,
            Err("notConnected".into())
        );
        assert_eq!(
            likes(&dir, &alice, &stranger),
            Ok(Likes {
                ours: false,
                matched: false,
                passed: false
            })
        );
    }

    #[test]
    fn chat_reads_are_identity_bound_and_hide_blocked_peers() {
        use dyapp_client::interactions::Interaction;
        let (dir, alice, bob) = (temp_dir(), Identity::generate(), Identity::generate());
        let alice_id = alice.peer_id();
        let bob_id = bob.peer_id();
        let mut store = open(&dir, &alice).unwrap();
        for (sender, target, device, event_id, wall_ms) in [
            (&alice_id, &bob_id, [1; 32], [1; 16], 1_000),
            (&bob_id, &alice_id, [2; 32], [2; 16], 2_000),
        ] {
            store
                .apply_interaction(
                    sender,
                    target,
                    &device,
                    Interaction {
                        kind: Kind::Like,
                        event_id,
                        hlc: wall_ms << 16,
                    },
                )
                .unwrap();
        }
        drop(store);
        assert_eq!(chat_rows(&dir, &alice).unwrap().len(), 1);
        let row = &chat_rows(&dir, &alice).unwrap()[0];
        assert_eq!(row.peer_id, bob_id);
        assert_eq!(row.state, view::ChatState::Open);
        assert_eq!(row.last_message, None);
        assert_eq!(row.unread_count, 0);
        assert!(chat_history(&dir, &alice, &bob_id)
            .unwrap()
            .messages
            .is_empty());
        assert!(chat_rows(&dir, &bob).is_err());
        assert_eq!(clear_chat(&dir, &alice, &bob_id), Ok(0));
        assert_eq!(mark_chat_read(&dir, &alice, &bob_id), Ok(0));
        assert_eq!(chat_rows(&dir, &alice).unwrap().len(), 1);
        assert_eq!(clear_chat(&dir, &alice, &alice_id), Err("ownPeer".into()));
        assert_eq!(
            mark_chat_read(&dir, &alice, &alice_id),
            Err("ownPeer".into())
        );
        let unknown = Identity::generate().peer_id();
        assert_eq!(
            clear_chat(&dir, &alice, &unknown),
            Err("unknownPeer".into())
        );
        assert_eq!(
            mark_chat_read(&dir, &alice, &unknown),
            Err("unknownPeer".into())
        );
        let store = open(&dir, &alice).unwrap();
        store.set_blocked(&bob_id, true).unwrap();
        assert!(chat_rows(&dir, &alice).unwrap().is_empty());
        assert_eq!(chat_history(&dir, &alice, &bob_id), Err("blocked".into()));
        assert_eq!(clear_chat(&dir, &alice, &bob_id), Err("blocked".into()));
        assert_eq!(mark_chat_read(&dir, &alice, &bob_id), Err("blocked".into()));
    }

    #[tokio::test]
    async fn text_send_rejects_bad_local_state_before_network() {
        let dir = temp_dir();
        let owner = Identity::generate();
        let device = Identity::generate();
        let peer = Identity::generate().peer_id();
        let cache = Mutex::default();
        assert_eq!(
            send_text(
                &cache,
                &dir,
                &dir,
                &owner,
                &device,
                &owner.peer_id(),
                "hello"
            )
            .await,
            Err("ownPeer".into())
        );
        assert_eq!(
            send_text(&cache, &dir, &dir, &owner, &device, &peer, "").await,
            Err("invalidText".into())
        );
        assert_eq!(
            send_text(&cache, &dir, &dir, &owner, &device, &peer, "hello").await,
            Err("unpublished".into())
        );
        open(&dir, &owner)
            .unwrap()
            .set_blocked(&peer, true)
            .unwrap();
        assert_eq!(
            send_text(&cache, &dir, &dir, &owner, &device, &peer, "hello").await,
            Err("blocked".into())
        );
    }

    #[test]
    fn send_uses_only_a_current_signed_peer_profile() {
        use dyapp_profile::Profile;
        let owner = Identity::generate();
        let peer_id = owner.peer_id();
        let signed = |version| {
            dyapp_profile::verify(
                &Profile {
                    version,
                    ..Default::default()
                }
                .sign(&owner),
            )
            .unwrap()
        };
        let cache = Mutex::default();
        assert_eq!(
            accept_peer(&cache, 0, &peer_id, signed(2))
                .unwrap()
                .profile
                .version,
            2
        );
        assert_eq!(
            accept_peer(&cache, 0, &peer_id, signed(1)),
            Err("staleProfile".into())
        );
        let other = Identity::generate().peer_id();
        assert_eq!(
            accept_peer(&cache, 0, &other, signed(3)),
            Err("unknownPeer".into())
        );
        cache.lock().unwrap().clear();
        assert_eq!(
            accept_peer(&cache, 0, &peer_id, signed(3)),
            Err("notConnected".into())
        );
    }
}
