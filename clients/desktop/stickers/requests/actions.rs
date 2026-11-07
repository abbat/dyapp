//! Queue first; the common mailbox pass delivers requests even if this action runs offline.
use super::*;
use crate::profile;
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

fn current_job(
    store: &Store,
    selection: &Selection,
    pack_id: [u8; 32],
    expected: Option<[u8; 16]>,
) -> Result<PackDownloadJob, String> {
    let pending = store
        .pending_sticker_pack(&selection.peer_id, &pack_id)
        .map_err(|e| e.to_string())?
        .ok_or("requestChanged")?;
    if expected != Some(pending.token) {
        return Err("requestChanged".into());
    }
    Ok(PackDownloadJob {
        peer: selection.peer_id.clone(),
        pack_id,
        sticker_hash: pending.sticker_hash,
        token: pending.token,
    })
}
fn apply(
    mut store: Store,
    key: &[u8; 32],
    selection: &Selection,
    action: &Action,
) -> Result<(), String> {
    let item = chat::selected(&store, selection)?;
    if item.key.author != selection.peer_id {
        return Err("unknownMessage".into());
    }
    let job = match action.kind {
        Kind::Request => {
            if action.generation.is_some() {
                return Err("requestChanged".into());
            }
            // A second initial click must not enqueue another request or reset current intent.
            if status(&mut store, key, selection)?.state != "new" {
                return Ok(());
            }
            None
        }
        _ => Some(current_job(
            &store,
            selection,
            item.sticker.pack_id,
            action.generation,
        )?),
    };
    if let Some(job) = job.as_ref() {
        match action.kind {
            Kind::Retry => {
                return store
                    .retry_sticker_pack_download(job)
                    .map_err(|e| e.to_string())?
                    .map(|_| ())
                    .ok_or_else(|| "requestChanged".into());
            }
            Kind::StopWaiting => {
                return if store
                    .finish_sticker_pack(&job.peer, &job.pack_id, &job.token)
                    .map_err(|e| e.to_string())?
                {
                    Ok(())
                } else {
                    Err("requestChanged".into())
                };
            }
            _ => {}
        }
    }
    let own = profile::stored(&store)?;
    if own.version == 0 {
        return Err("unpublished".into());
    }
    let mut session = Session::open(store, key).map_err(|e| e.to_string())?;
    if !own
        .devices
        .contains(&session.certificate().map_err(|e| e.to_string())?)
    {
        return Err("unpublished".into());
    }
    match job {
        Some(job) => {
            session
                .stage_fresh_sticker_pack_request(&job, key)
                .map_err(|e| e.to_string())?;
        }
        None => {
            session
                .stage_sticker_pack_request(&selection.peer_id, &item.key, key)
                .map_err(|e| e.to_string())?;
        }
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
