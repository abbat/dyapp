//! Desktop own-message operations bound to their selected peer and MLS group.

use std::path::Path;
use std::sync::Arc;

use dyapp_client::chat::MAX_TEXT_BYTES;
use dyapp_client::message_actions::{self, Kind, Source};
use dyapp_client::session::Session;
#[cfg(test)]
use dyapp_client::store::messages::{ChatItem, ChatMessage, MessageKey};
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use crate::profile;

// Keep the original regression assertions connected to the common matcher.
#[cfg(test)]
fn target_matches(message: &ChatMessage, own: &str, group: &[u8], id: [u8; 16]) -> bool {
    item_matches(ChatItem::Text(message.clone()), own, group, id, false)
}

#[cfg(test)]
fn item_matches(item: ChatItem, own: &str, group: &[u8], id: [u8; 16], delete: bool) -> bool {
    message_actions::matches(
        &item,
        &MessageKey {
            group: group.to_vec(),
            author: own.to_owned(),
            id,
        },
        if delete {
            Kind::TextOrSticker
        } else {
            Kind::Text
        },
    )
}

fn validate_target(
    store: &Store,
    identity: &Identity,
    peer_id: &str,
    group_id: &[u8],
    message_id: [u8; 16],
    delete: bool,
) -> Result<(), String> {
    message_actions::prepare(
        store,
        Source {
            peer: peer_id,
            group: group_id,
            author: &identity.peer_id(),
            id: &message_id,
        },
        if delete {
            Kind::TextOrSticker
        } else {
            Kind::Text
        },
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[expect(
    clippy::too_many_arguments,
    reason = "keep storage paths, author/device keys and authenticated message key explicit at the operation boundary"
)]
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
    let mut session = Session::open_or_create_with_scores(
        store,
        identity,
        device_key,
        &seal_key,
        dyapp_client::contact_verification::scores(dir).map_err(|e| e.to_string())?,
    )
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

    fn sticker_fixture(own: &str) -> dyapp_client::store::messages::ChatSticker {
        dyapp_client::store::messages::ChatSticker {
            key: MessageKey {
                group: vec![4],
                author: own.to_owned(),
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
        }
    }

    #[test]
    fn stickers_are_delete_only_and_require_the_exact_own_key() {
        let own = Identity::try_generate().unwrap().peer_id();
        let message = sticker_fixture(&own);
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
        let own = Identity::try_generate().unwrap().peer_id();
        let peer = Identity::try_generate().unwrap().peer_id();
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
