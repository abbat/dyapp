//! Explicit profile openings: opaque UI handles, with disclosure policy in the common core.
use crate::search::SearchState;
use dyapp_client::contact_verification::Context;
use dyapp_client::interactions::{profile_view::Opening, Kind};
use dyapp_client::session::Submission;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct Capture {
    dir: PathBuf,
    context: Arc<Mutex<Context>>,
    opening: Opening,
}

#[derive(Default)]
struct Handles {
    next: u64,
    current: Option<(String, Arc<Capture>)>,
}

/// One idle opening holds neither an identity key nor an admission lease.
#[derive(Clone, Default)]
pub(crate) struct ViewState(Arc<Mutex<Handles>>);

impl ViewState {
    fn prepare(
        &self,
        context: Arc<Mutex<Context>>,
        identity: &Identity,
        dir: &Path,
        peer: String,
    ) -> Result<String, String> {
        let opening = {
            let held = context.lock().map_err(|error| error.to_string())?;
            Opening::new(&held, identity.peer_id(), peer)
        };
        let mut handles = self.0.lock().map_err(|error| error.to_string())?;
        let next = handles
            .next
            .checked_add(1)
            .ok_or("Profile view handle exhausted")?;
        let token = next.to_string();
        if let Some((_, previous)) = &handles.current {
            previous.opening.retire();
        }
        handles.next = next;
        handles.current = Some((
            token.clone(),
            Arc::new(Capture {
                dir: dir.into(),
                context,
                opening,
            }),
        ));
        Ok(token)
    }

    fn capture(&self, token: &str, dir: &Path) -> Result<Arc<Capture>, String> {
        let handles = self.0.lock().map_err(|error| error.to_string())?;
        handles
            .current
            .as_ref()
            .filter(|(current, capture)| current == token && capture.dir == dir)
            .map(|(_, capture)| Arc::clone(capture))
            .ok_or_else(|| "Profile view opening changed".into())
    }

    fn retire(&self, token: &str) -> Result<(), String> {
        let mut handles = self.0.lock().map_err(|error| error.to_string())?;
        if let Some((_, capture)) = handles
            .current
            .as_ref()
            .filter(|(current, _)| current == token)
        {
            capture.opening.retire();
            handles.current = None;
        }
        Ok(())
    }
}

/// Immediate node delivery of a committed event, never a peer receipt.
#[derive(serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub(crate) enum Delivery {
    Accepted { id: [u8; 16] },
    Queued { id: [u8; 16], detail: String },
}

impl From<Submission> for Delivery {
    fn from(submission: Submission) -> Self {
        match submission {
            Submission::Accepted { id } => Self::Accepted { id },
            Submission::Queued { id, error } => Self::Queued {
                id,
                detail: error.to_string(),
            },
        }
    }
}

async fn send(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    capture: &Capture,
) -> Result<Option<Delivery>, String> {
    let store = super::open(dir, identity)?;
    let active = {
        let context = capture.context.lock().map_err(|error| error.to_string())?;
        if !capture
            .opening
            .admit_at(&context, &store, dyapp_client::clock::now_ms())
            .map_err(|error| error.to_string())?
        {
            return Ok(None);
        }
        // Activation uses this same context lock through its active-settings write.
        network::load(dir, Settings::Active)
            .map_err(|error| error.to_string())?
            .ok_or("notConnected")?
    };
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let device = crate::device_key(root, dir, identity)?;
    let mut session = super::shared_session(store, identity, &device, &seal_key, dir)?;
    let own = super::share::share_owner(&session, identity)?;
    let mut node = crate::profile::connect(&active).await?;
    let peer = node
        .get_profile(capture.opening.peer())
        .await
        .map_err(|error| error.to_string())?;
    let committed = session
        .submit_interaction_with(
            std::slice::from_mut(&mut node),
            &[own, peer],
            Kind::View,
            &seal_key,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(committed.map(Submission::from).map(Delivery::from))
}

/// Capture an explicit opening before scheduling its network work; no disclosure occurs.
#[tauri::command]
pub(crate) async fn prepare_profile_view(
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, SearchState>,
    views: tauri::State<'_, ViewState>,
    peer_id: String,
) -> Result<String, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let cache = Arc::clone(&searching.0);
    let views = views.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let context = {
            let mut cache = cache.lock().map_err(|error| error.to_string())?;
            cache.bind(&dir)?;
            Arc::clone(&cache.context)
        };
        views.prepare(context, &identity, &dir, peer_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Submit the exact opening, retaining the identity lease through completion.
#[tauri::command]
pub(crate) async fn submit_profile_view(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    sessions: tauri::State<'_, super::Sessions>,
    views: tauri::State<'_, ViewState>,
    token: String,
) -> Result<Option<Delivery>, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let capture = views.capture(&token, &dir)?;
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|error| error.to_string())?;
        tauri::async_runtime::block_on(send(&root, &dir, &identity, &capture))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Retire only the named opening, including when its preparation finishes after UI disposal.
#[tauri::command]
pub(crate) async fn retire_profile_view(
    views: tauri::State<'_, ViewState>,
    token: String,
) -> Result<(), String> {
    let views = views.inner().clone();
    tauri::async_runtime::spawn_blocking(move || views.retire(&token))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
#[path = "profile_view/tests.rs"]
mod tests;
