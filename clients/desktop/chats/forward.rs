//! Forward a current local text into another open conversation as this identity's own words.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::forwarding::{self, Source};
use dyapp_client::session::{Session, Submission};
use dyapp_client::store::{Conversation, Store};
use dyapp_client::Node;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use crate::profile;
use crate::search::ProfileCache;

/// The Session, node and sealing key for a send into `target_peer`, once the chat is open and
/// unblocked and both profiles are published.
pub(crate) async fn open_session(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    store: Store,
    target_peer: &str,
) -> Result<(Session, Node, zeroize::Zeroizing<[u8; 32]>), String> {
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
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    if !own
        .profile
        .devices
        .contains(&session.certificate().map_err(|e| e.to_string())?)
    {
        return Err("unpublished".into());
    }
    Ok((session, node, seal_key))
}

/// A committed forward: `queued` names the delivery failure; the core keeps the exact bytes for retry.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Forwarded {
    id: [u8; 16],
    queued: Option<String>,
}

impl From<Submission> for Forwarded {
    fn from(submission: Submission) -> Self {
        match submission {
            Submission::Accepted { id } => Self { id, queued: None },
            Submission::Queued { id, error } => Self {
                id,
                queued: Some(error.to_string()),
            },
        }
    }
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
) -> Result<Forwarded, String> {
    let store = super::open(dir, identity)?;
    let text = forwarding::prepare(
        &store,
        Source {
            peer: source_peer,
            group: source_group,
            author: source_author,
            id: &source_message,
        },
        target_peer,
    )
    .map_err(|error| error.to_string())?;
    let (mut session, mut node, seal_key) =
        open_session(cache, root, dir, identity, device_key, store, target_peer).await?;
    session
        .submit_forwarded_text_to(
            std::slice::from_mut(&mut node),
            target_peer,
            &text,
            &seal_key,
        )
        .await
        .map(Forwarded::from)
        .map_err(|e| e.to_string())
}

/// Forward the selected live text through the active identity's MLS Session.
///
/// An error means nothing was committed; a queued result must not be submitted again.
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
) -> Result<Forwarded, String> {
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
    use super::{Forwarded, Submission};

    #[test]
    fn queued_forward_keeps_committed_id_for_ipc() {
        let id = [7; 16];
        let queued = Forwarded::from(Submission::Queued {
            id,
            error: dyapp_client::session::Error::Invalidated,
        });
        assert_eq!(
            serde_json::to_value(&queued).unwrap(),
            serde_json::json!({ "id": id, "queued": "MLS session must be reopened" })
        );
        let accepted = Forwarded::from(Submission::Accepted { id });
        assert_eq!(
            serde_json::to_value(&accepted).unwrap(),
            serde_json::json!({ "id": id, "queued": null })
        );
    }
}
