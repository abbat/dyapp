//! Forward a current local text into another open conversation as this identity's own words.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::chat::MAX_TEXT_BYTES;
use dyapp_client::session::Session;
use dyapp_client::store::messages::ChatMessage;
use dyapp_client::store::Conversation;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use crate::profile;
use crate::search::ProfileCache;

fn source_text(
    messages: &[ChatMessage],
    group_id: &[u8],
    author: &str,
    message_id: [u8; 16],
) -> Result<Vec<u8>, String> {
    messages
        .iter()
        .find(|message| {
            message.key.group.as_slice() == group_id
                && message.key.author.as_str() == author
                && message.key.id == message_id
                && !message.current.deleted
        })
        .and_then(|message| message.current.text.clone())
        .filter(|text| !text.is_empty() && text.len() <= MAX_TEXT_BYTES)
        .ok_or("unknownMessage".into())
}

#[expect(
    clippy::too_many_arguments,
    reason = "source and target are distinct chat identities"
)]
async fn forward(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    source_peer: &str,
    source_group: &[u8],
    source_author: &str,
    source_message: [u8; 16],
    target_peer: &str,
) -> Result<[u8; 16], String> {
    if source_peer == target_peer || target_peer == identity.peer_id() {
        return Err("invalidTarget".into());
    }
    let mut store = super::open_visible_chat(dir, identity, source_peer)?;
    let text = source_text(
        &store
            .current_chat_messages(source_peer)
            .map_err(|e| e.to_string())?,
        source_group,
        source_author,
        source_message,
    )?;
    if store.blocked(target_peer).map_err(|e| e.to_string())? {
        return Err("blocked".into());
    }
    if store.conversation(target_peer).map_err(|e| e.to_string())? != Conversation::Open {
        return Err("invalidTarget".into());
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
    let fetched = node
        .get_profile(target_peer)
        .await
        .map_err(|e| e.to_string())?;
    let target = super::accept_peer(cache, epoch, target_peer, fetched)?;
    if target.profile.deleted || target.profile.version == 0 {
        return Err("unpublished".into());
    }
    let seal_key = crate::sealing_key(root, dir, &mut store, device_key)?;
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
        .send_forwarded_text_to(
            std::slice::from_mut(&mut node),
            target_peer,
            &text,
            &seal_key,
        )
        .await
        .map_err(|e| e.to_string())
}

/// Forward the selected live text through the active identity's MLS Session.
#[expect(
    clippy::too_many_arguments,
    reason = "Tauri command names the source and target message"
)]
#[tauri::command]
pub(crate) async fn forward_chat_text(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, crate::search::SearchState>,
    sessions: tauri::State<'_, super::Sessions>,
    source_peer_id: String,
    source_group_id: Vec<u8>,
    source_author: String,
    source_message_id: [u8; 16],
    target_peer_id: String,
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
        tauri::async_runtime::block_on(forward(
            &cache,
            &root,
            &dir,
            &identity,
            &device,
            &source_peer_id,
            &source_group_id,
            &source_author,
            source_message_id,
            &target_peer_id,
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
    fn forward_source_must_match_live_message_and_author() {
        let message = ChatMessage {
            key: MessageKey {
                group: vec![1],
                author: "sender".into(),
                id: [2; 16],
            },
            current: CurrentMessage {
                text: Some(b"words".to_vec()),
                edited: false,
                deleted: false,
            },
            original_hlc: None,
            unread: false,
            forwarded: false,
            delivery: None,
        };
        assert_eq!(
            source_text(&[message.clone()], &[1], "sender", [2; 16]),
            Ok(b"words".to_vec())
        );
        assert!(source_text(&[message.clone()], &[1], "other", [2; 16]).is_err());
        assert!(source_text(&[message.clone()], &[3], "sender", [2; 16]).is_err());
        assert!(source_text(&[message.clone()], &[1], "sender", [3; 16]).is_err());
        let mut deleted = message;
        deleted.current = CurrentMessage {
            text: None,
            edited: false,
            deleted: true,
        };
        assert!(source_text(&[deleted], &[1], "sender", [2; 16]).is_err());
    }
}
