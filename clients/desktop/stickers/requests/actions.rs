//! Queue first; the common mailbox pass delivers requests even if this action runs offline.
use super::{chats, owner_matches, pack_requests, status, IdentityState, Selection, Status, Store};
use dyapp_client::session::Session;
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    Request,
    Retry,
    Fresh,
    StopWaiting,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Action {
    kind: Kind,
    generation: Option<[u8; 16]>,
}

fn apply(
    mut store: Store,
    key: &[u8; 32],
    selection: &Selection,
    action: &Action,
) -> Result<(), String> {
    let action = pack_requests::Action {
        kind: match action.kind {
            Kind::Request => pack_requests::Kind::Request,
            Kind::Retry => pack_requests::Kind::Retry,
            Kind::Fresh => pack_requests::Kind::Fresh,
            Kind::StopWaiting => pack_requests::Kind::StopWaiting,
        },
        generation: action.generation,
    };
    if let Some(stage) = pack_requests::prepare(
        &mut store,
        key,
        &selection.peer_id,
        &selection.key(),
        action,
    )
    .map_err(|e| e.to_string())?
    {
        let mut session = Session::open(store, key).map_err(|e| e.to_string())?;
        stage.stage(&mut session, key).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Local staging succeeds offline. Stop waiting retires acceptance, never recalls queued packets.
#[tauri::command]
pub(crate) async fn change_sticker_pack_request(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    selection: Selection,
    action: Action,
) -> Result<Status, String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let root = crate::config_dir(&app)?;
    let sessions = sessions.inner().0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = sessions.lock().map_err(|_| "Session state unavailable")?;
        let (identity, dir) = lease.active();
        if lease.transitioning().unwrap_or(true) {
            return Err("Identity changed".into());
        }
        let store = chats::open(dir, identity)?;
        let key = crate::sealing_key(&root, dir, &store)?;
        apply(store, &key, &selection, &action)?;
        let mut store = chats::open(dir, identity)?;
        status(&mut store, &key, &selection)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;
