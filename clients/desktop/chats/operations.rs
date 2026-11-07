//! Desktop own-message operations bound to their selected peer and MLS group.

use std::path::Path;
use std::sync::Arc;

use dyapp_client::chat::MAX_TEXT_BYTES;
use dyapp_client::session::Session;
use dyapp_client::store::messages::{ChatItem, ChatMessage};
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use crate::profile;

fn target_matches(message: &ChatMessage, own: &str, group_id: &[u8], id: [u8; 16]) -> bool {
    message.key.group.as_slice() == group_id
        && message.key.id == id
        && message.key.author == own
        && !message.current.deleted
}

fn item_matches(item: ChatItem, own: &str, group: &[u8], id: [u8; 16], delete: bool) -> bool {
    match item {
        ChatItem::Text(message) => target_matches(&message, own, group, id),
        ChatItem::Sticker(message) => {
            delete
                && message.key.author == own
                && message.key.group == group
                && message.key.id == id
        }
        _ => false,
    }
}

fn validate_target(
    store: &Store,
    identity: &Identity,
    peer_id: &str,
    group_id: &[u8],
    message_id: [u8; 16],
    delete: bool,
) -> Result<(), String> {
    if peer_id == identity.peer_id() {
        return Err("ownPeer".into());
    }
    if store.blocked(peer_id).map_err(|e| e.to_string())? {
        return Err("blocked".into());
    }
    let own = identity.peer_id();
    let mut found = store
        .current_chat_messages(peer_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .any(|message| item_matches(ChatItem::Text(message), &own, group_id, message_id, delete));
    if !found && delete {
        found = store
            .current_chat_stickers(peer_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .any(|message| {
                item_matches(ChatItem::Sticker(message), &own, group_id, message_id, true)
            });
    }
    if !found {
        return Err("unknownMessage".into());
    }
    Ok(())
}

async fn operate(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    peer_id: &str,
    group_id: &[u8],
    message_id: [u8; 16],
    text: Option<&str>,
) -> Result<[u8; 16], String> {
    if let Some(text) = text {
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err("invalidText".into());
        }
    }
    let store = super::open(dir, identity)?;
    validate_target(
        &store,
        identity,
        peer_id,
        group_id,
        message_id,
        text.is_none(),
    )?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    match text {
        Some(text) => session
            .send_edit(
                std::slice::from_mut(&mut node),
                group_id,
                message_id,
                text.as_bytes(),
                &seal_key,
            )
            .await
            .map_err(|e| e.to_string()),
        None => session
            .send_delete(
                std::slice::from_mut(&mut node),
                group_id,
                message_id,
                &seal_key,
            )
            .await
            .map_err(|e| e.to_string()),
    }
}

/// Edit only a live text authored by this identity in the selected peer's group.
#[tauri::command]
pub(crate) async fn edit_chat_text(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    sessions: tauri::State<'_, super::Sessions>,
    peer_id: String,
    group_id: Vec<u8>,
    message_id: [u8; 16],
    text: String,
) -> Result<[u8; 16], String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        let device = crate::device_key(&root, &dir, &identity)?;
        tauri::async_runtime::block_on(operate(
            &root,
            &dir,
            &identity,
            &device,
            &peer_id,
            &group_id,
            message_id,
            Some(&text),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_client::store::messages::{CurrentMessage, MessageKey};

    #[test]
    fn stickers_are_delete_only_and_require_the_exact_own_key() {
        let own = Identity::generate().peer_id();
        let message = dyapp_client::store::messages::ChatSticker {
            key: MessageKey {
                group: vec![4],
                author: own.clone(),
                id: [2; 16],
            },
            sticker: dyapp_client::chat::sticker::Sticker {
                id: [2; 16],
                hlc: 1,
                pack_id: [3; 32],
                sha256: [4; 32],
                emoji: "😺".into(),
                pack_name: "Cats".into(),
                attachment: None,
            },
            unread: false,
            delivery: None,
        };
        assert!(item_matches(
            ChatItem::Sticker(message.clone()),
            &own,
            &[4],
            [2; 16],
            true
        ));
        assert!(!item_matches(
            ChatItem::Sticker(message.clone()),
            &own,
            &[4],
            [2; 16],
            false
        ));
        assert!(!item_matches(
            ChatItem::Sticker(message.clone()),
            &own,
            &[5],
            [2; 16],
            true
        ));
        assert!(!item_matches(
            ChatItem::Sticker(message.clone()),
            &own,
            &[4],
            [9; 16],
            true
        ));
        assert!(!item_matches(
            ChatItem::Sticker(message),
            "another author",
            &[4],
            [2; 16],
            true
        ));
    }

    #[test]
    fn edit_target_is_a_live_own_text_in_the_selected_group() {
        let own = Identity::generate().peer_id();
        let peer = Identity::generate().peer_id();
        let mut message = ChatMessage {
            key: MessageKey {
                group: vec![4],
                author: own.clone(),
                id: [2; 16],
            },
            current: CurrentMessage {
                text: Some(b"text".to_vec()),
                edited: false,
                deleted: false,
            },
            original_hlc: None,
            unread: false,
            forwarded: false,
            delivery: None,
        };
        assert!(target_matches(&message, &own, &[4], [2; 16]));
        assert!(!target_matches(&message, &own, &[5], [2; 16]));
        assert!(!target_matches(&message, &own, &[4], [3; 16]));
        message.key.author = peer;
        assert!(!target_matches(&message, &own, &[4], [2; 16]));
        message.key.author = own.clone();
        message.current.deleted = true;
        assert!(!target_matches(&message, &own, &[4], [2; 16]));
    }
}

/// Delete only a live text or sticker authored by this identity in the selected peer's group.
#[tauri::command]
pub(crate) async fn delete_chat_text(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    sessions: tauri::State<'_, super::Sessions>,
    peer_id: String,
    group_id: Vec<u8>,
    message_id: [u8; 16],
) -> Result<[u8; 16], String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        let device = crate::device_key(&root, &dir, &identity)?;
        tauri::async_runtime::block_on(operate(
            &root, &dir, &identity, &device, &peer_id, &group_id, message_id, None,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
