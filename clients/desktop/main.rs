use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dyapp_identity::Identity;
use network::{Draft, NetworkState};
use profile::ProfileState;
use slots::{Opened, Slot};
use tauri::webview::PageLoadEvent;
use tauri::Manager;

#[path = "file_store.rs"]
mod file_store;
#[path = "network.rs"]
mod network;
#[path = "profile.rs"]
mod profile;
#[path = "search.rs"]
mod search;
#[path = "slots.rs"]
mod slots;
#[path = "system_store.rs"]
mod system_store;

/// The open identity and its slot folder.
struct IdentityState(Mutex<Option<(Arc<Identity>, PathBuf)>>);

fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if std::env::args().any(|arg| arg == "--ui-test") {
        if let Some(path) = std::env::var_os("DYAPP_TEST_CONFIG_DIR") {
            return Ok(PathBuf::from(path));
        }
    }
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn load(slot: &Slot) -> Result<Option<Identity>, String> {
    match slot.mode {
        Some("system") => system_store::load_slot(&slot.system_name()),
        Some(_) => match fs::symlink_metadata(slot.key_path()) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            _ => file_store::load(&slot.key_path()).map(Some),
        },
        None => Err("Unknown identity storage mode".into()),
    }
}

fn save(slot: &Slot, identity: &Identity) -> Result<(), String> {
    match slot.mode {
        Some("system") => system_store::save_slot(&slot.system_name(), identity),
        Some(_) => file_store::save(&slot.key_path(), identity),
        None => Err("Unknown identity storage mode".into()),
    }
}

fn erase(slot: &Slot) -> Result<(), String> {
    match slot.mode {
        Some("system") => system_store::delete_slot(&slot.system_name()),
        Some(_) => match fs::remove_file(slot.key_path()) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result.map_err(|e| e.to_string()),
        },
        None => Err("Unknown identity storage mode".into()),
    }
}

fn ready_result(mode: &str, peer_id: String, dir: &Path) -> HashMap<&'static str, String> {
    let mut result = HashMap::from([
        ("status", "ready".into()),
        ("mode", mode.to_owned()),
        ("peerId", peer_id),
    ]);
    if mode == "file" {
        result.insert("keyPath", dir.join("identity.key").display().to_string());
    }
    result
}

/// The page's view of an opened slot and the identity it unlocked, if any.
fn describe(opened: Opened) -> (HashMap<&'static str, String>, Option<(Identity, PathBuf)>) {
    let mut result = HashMap::new();
    match opened {
        Opened::Ready(slot, identity) => {
            result = ready_result(slot.mode.unwrap_or_default(), identity.peer_id(), &slot.dir);
            result.insert("slot", slot.id);
            return (result, Some((identity, slot.dir)));
        }
        Opened::Setup => {
            result.insert("status", "setup".into());
        }
        Opened::Picker => {
            result.insert("status", "picker".into());
        }
        Opened::Recovery(slot, reason, peer_id) => {
            result.insert("status", "recovery".into());
            result.insert("reason", reason.into());
            result.insert("slot", slot.id);
            result.extend(slot.mode.map(|mode| ("mode", mode.into())));
            result.extend(peer_id.map(|peer_id| ("peerId", peer_id)));
        }
    }
    (result, None)
}

/// Makes `opened` the current identity; a picker or recovery state leaves none open.
fn respond(opened: Opened, state: &IdentityState) -> Result<HashMap<&'static str, String>, String> {
    let (result, identity) = describe(opened);
    *state.0.lock().map_err(|e| e.to_string())? =
        identity.map(|(identity, dir)| (Arc::new(identity), dir));
    Ok(result)
}

#[tauri::command]
async fn identity_state(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
) -> Result<HashMap<&'static str, String>, String> {
    let dir = config_dir(&app)?;
    let opened = tauri::async_runtime::spawn_blocking(move || slots::open(&dir, &load))
        .await
        .map_err(|e| e.to_string())??;
    if let Opened::Setup = opened {
        return storage_options().await;
    }
    respond(opened, &state)
}

/// The storage choice for a new identity.
#[tauri::command]
async fn storage_options() -> Result<HashMap<&'static str, String>, String> {
    let availability = tauri::async_runtime::spawn_blocking(system_store::probe)
        .await
        .map_err(|e| e.to_string())?;
    let mut result = describe(Opened::Setup).0;
    match availability {
        Ok(()) => {
            result.insert("systemAvailable", "true".into());
        }
        Err(reason) => {
            result.insert("systemAvailable", "false".into());
            result.insert("systemReason", reason.key().into());
        }
    }
    Ok(result)
}

/// Every local identity as the picker shows it; listing opens keys but changes nothing.
#[tauri::command]
async fn list_identities(
    app: tauri::AppHandle,
) -> Result<Vec<HashMap<&'static str, String>>, String> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(slots::list(&dir)?
            .into_iter()
            .map(|slot| {
                let (id, mode) = (slot.id.clone(), slot.mode);
                let mut result = match slots::open_slot(slot, &load) {
                    Ok(opened) => describe(opened).0,
                    Err(_) => HashMap::from([
                        ("status", "recovery".into()),
                        ("reason", "unavailable".into()),
                    ]),
                };
                result.insert("slot", id);
                result.extend(mode.map(|mode| ("mode", mode.into())));
                result
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn select_identity(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    slot: String,
) -> Result<HashMap<&'static str, String>, String> {
    let dir = config_dir(&app)?;
    let opened = tauri::async_runtime::spawn_blocking(move || {
        let slot = slots::list(&dir)?
            .into_iter()
            .find(|listed| listed.id == slot)
            .ok_or("Unknown identity")?;
        slots::set_active(&dir, &slot.id)?;
        slots::open_slot(slot, &load)
    })
    .await
    .map_err(|e| e.to_string())??;
    respond(opened, &state)
}

#[tauri::command]
async fn create_identity(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    mode: String,
) -> Result<HashMap<&'static str, String>, String> {
    let mode = match mode.as_str() {
        "system" => "system",
        "file" => "file",
        _ => return Err("Unknown identity storage mode".into()),
    };
    let dir = config_dir(&app)?;
    let (slot, identity) = tauri::async_runtime::spawn_blocking(move || {
        if mode == "system" {
            system_store::probe()
                .map_err(|reason| format!("System storage unavailable: {:?}", reason))?;
        }
        slots::create(&dir, mode, &save, &load)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut response = ready_result(mode, identity.peer_id(), &slot.dir);
    response.insert("slot", slot.id);
    *state.0.lock().map_err(|e| e.to_string())? = Some((Arc::new(identity), slot.dir));
    Ok(response)
}

/// Removes a slot whose key yields the Peer ID the user confirmed, then shows the picker, or the
/// storage choice when no identity is left, so removal never switches identities silently.
#[tauri::command]
async fn remove_identity(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    slot: String,
    peer_id: Option<String>,
) -> Result<HashMap<&'static str, String>, String> {
    let dir = config_dir(&app)?;
    let remaining = tauri::async_runtime::spawn_blocking(move || {
        slots::remove(&dir, &slot, peer_id.as_deref(), &load, &erase)?;
        slots::list(&dir).map(|slots| slots.len())
    })
    .await
    .map_err(|e| e.to_string())??;
    *state.0.lock().map_err(|e| e.to_string())? = None;
    if remaining == 0 {
        return storage_options().await;
    }
    Ok(describe(Opened::Picker).0)
}

fn open_identity(state: &IdentityState) -> Result<(Arc<Identity>, PathBuf), String> {
    let open = state.0.lock().map_err(|e| e.to_string())?;
    open.clone().ok_or_else(|| "No identity is open".into())
}

fn identity_dir(state: &IdentityState) -> Result<PathBuf, String> {
    open_identity(state).map(|(_, dir)| dir)
}

/// The open identity's `draft` or `active` bootstrap settings, `null` when never saved.
#[tauri::command]
async fn load_network(
    state: tauri::State<'_, IdentityState>,
    file: String,
) -> Result<Option<Draft>, String> {
    let dir = identity_dir(&state)?;
    tauri::async_runtime::spawn_blocking(move || network::load(&dir, &file))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_network_draft(
    state: tauri::State<'_, IdentityState>,
    draft: Draft,
) -> Result<(), String> {
    let dir = identity_dir(&state)?;
    tauri::async_runtime::spawn_blocking(move || network::save_draft(&dir, draft))
        .await
        .map_err(|e| e.to_string())?
}

/// Saves the draft and reports every seed row; see `network::check` for the errors.
#[tauri::command]
async fn check_connection(
    state: tauri::State<'_, IdentityState>,
    checks: tauri::State<'_, NetworkState>,
    draft: Draft,
    wait_ms: u32,
) -> Result<Vec<HashMap<&'static str, String>>, String> {
    let dir = identity_dir(&state)?;
    network::check(&checks, dir, draft, Duration::from_millis(wait_ms.into())).await
}

#[tauri::command]
fn cancel_connection_check(checks: tauri::State<'_, NetworkState>) {
    network::cancel(&checks);
}

#[tauri::command]
async fn activate_connection(
    state: tauri::State<'_, IdentityState>,
    checks: tauri::State<'_, NetworkState>,
) -> Result<Draft, String> {
    let dir = identity_dir(&state)?;
    network::activate(&checks, &dir)
}

/// Picker values from the shared profile vocabulary.
#[tauri::command]
fn profile_scales() -> Vec<profile::PickerScale> {
    profile::scales()
}

/// The open identity's profile draft; see `profile::load`.
#[tauri::command]
async fn load_profile(state: tauri::State<'_, IdentityState>) -> Result<profile::View, String> {
    let (identity, dir) = open_identity(&state)?;
    tauri::async_runtime::spawn_blocking(move || profile::load(&dir, &identity))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_profile(
    state: tauri::State<'_, IdentityState>,
    draft: profile::Draft,
) -> Result<profile::View, String> {
    let (identity, dir) = open_identity(&state)?;
    tauri::async_runtime::spawn_blocking(move || profile::save(&dir, &identity, draft))
        .await
        .map_err(|e| e.to_string())?
}

/// Runs the profile proof of work until solved or cancelled; see `profile::solve`.
#[tauri::command]
async fn solve_profile(
    state: tauri::State<'_, IdentityState>,
    solving: tauri::State<'_, ProfileState>,
    attested: bool,
) -> Result<(), String> {
    let (identity, dir) = open_identity(&state)?;
    let cancel = Arc::clone(&solving.0);
    cancel.store(false, std::sync::atomic::Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        profile::solve(&cancel, &dir, &identity, attested, profile::POW_BITS)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_profile_solve(solving: tauri::State<'_, ProfileState>) {
    solving.0.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Publishes the solved draft and checks the readback; see `profile::publish`.
#[tauri::command]
async fn publish_profile(
    state: tauri::State<'_, IdentityState>,
    attested: bool,
) -> Result<profile::View, String> {
    let (identity, dir) = open_identity(&state)?;
    // The client store is not `Sync`, so the publish runs on a thread of its own.
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(profile::publish(
            &dir,
            &identity,
            attested,
            profile::POW_BITS,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Verified profile search on the active network; see `search::search`.
#[tauri::command]
async fn search_profiles(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, search::SearchState>,
    conditions: Vec<search::Condition>,
    limit: u32,
) -> Result<search::Results, String> {
    let (identity, dir) = open_identity(&state)?;
    let cache = Arc::clone(&searching.0);
    // The client store is not `Sync`, so the search runs on a thread of its own.
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(search::search(&cache, &dir, &identity, conditions, limit))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether the page shows pseudo-localized strings (`--pseudo-locale`, practice D1).
#[tauri::command]
fn pseudo_locale() -> bool {
    std::env::args().any(|arg| arg == "--pseudo-locale")
}

#[tauri::command]
fn ui_test_russian() -> bool {
    std::env::args().any(|arg| arg == "--ui-test")
        && std::env::args().any(|arg| arg == "--ui-test-ru")
}

#[tauri::command]
fn ui_test_mode() -> String {
    if std::env::args().any(|arg| arg == "--ui-test-file") {
        "file".into()
    } else {
        "system".into()
    }
}

#[tauri::command]
fn ui_test_existing() -> bool {
    std::env::args().any(|arg| arg == "--ui-test-existing")
}

/// The extra flow a UI test drives: `add`, `pick`, `recovery`, `remove`, `network`,
/// `network-restart` or `connect:<seed>`.
#[tauri::command]
fn ui_test_step() -> String {
    std::env::args()
        .find_map(|arg| arg.strip_prefix("--ui-test-step=").map(str::to_owned))
        .unwrap_or_default()
}

#[tauri::command]
fn ui_test_result(
    app: tauri::AppHandle,
    window: tauri::Window,
    passed: bool,
    peer_id: String,
    reason: String,
) {
    if !std::env::args().any(|arg| arg == "--ui-test") {
        return;
    }
    let success = passed && window.is_visible().unwrap_or(false);
    if success {
        println!("UI_SMOKE_PASS:{peer_id}");
    } else {
        let reason = reason.lines().next().unwrap_or("unknown");
        println!(
            "UI_SMOKE_FAIL: {}",
            reason.chars().take(160).collect::<String>()
        );
    }
    app.exit(if success { 0 } else { 1 });
}

fn main() {
    let test_mode = std::env::args().any(|arg| arg == "--ui-test");
    // The isolated loopback smoke uses a cheap node-ID puzzle on both ends.
    if test_mode && std::env::var_os("DYAPP_TEST_CONFIG_DIR").is_some() {
        dyapp_p2p_net::set_id_pow_bits(8);
    }
    let break_ui = std::env::args().any(|arg| arg == "--break-ui");
    tauri::Builder::default()
        .manage(IdentityState(Mutex::new(None)))
        .manage(NetworkState::default())
        .manage(ProfileState::default())
        .manage(search::SearchState::default())
        .invoke_handler(tauri::generate_handler![
            identity_state,
            storage_options,
            list_identities,
            select_identity,
            create_identity,
            remove_identity,
            load_network,
            save_network_draft,
            check_connection,
            cancel_connection_check,
            activate_connection,
            profile_scales,
            load_profile,
            save_profile,
            solve_profile,
            cancel_profile_solve,
            publish_profile,
            search_profiles,
            pseudo_locale,
            ui_test_russian,
            ui_test_mode,
            ui_test_existing,
            ui_test_step,
            ui_test_result
        ])
        .on_page_load(move |webview, payload| {
            if test_mode && payload.event() == PageLoadEvent::Finished {
                if break_ui {
                    webview
                        .eval("document.getElementById('app-ready').style.display = 'none'")
                        .expect("inject UI failure");
                }
                webview
                    .eval(include_str!("ui-test.js"))
                    .expect("start UI test");
            }
        })
        .run(tauri::generate_context!())
        .expect("launch DYApp");
}

#[cfg(test)]
mod tests {
    use super::ready_result;
    use std::path::Path;

    #[test]
    fn backup_path_is_only_returned_for_file_storage() {
        let dir = Path::new("profile");
        let file = ready_result("file", "peer".into(), dir);
        assert_eq!(
            file["keyPath"],
            dir.join("identity.key").display().to_string()
        );
        let system = ready_result("system", "peer".into(), dir);
        assert!(!system.contains_key("keyPath"));
    }

    #[test]
    fn configured_window_is_visible_and_local() {
        let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let app = &context.config().app;
        assert_eq!(app.windows.len(), 1);
        assert!(app.windows[0].visible);
        assert_eq!(app.windows[0].title, "DYApp");
        assert!(matches!(app.windows[0].url, tauri::WebviewUrl::App(_)));
        // No capabilities: the page can call only the app's own command, no plugin APIs.
        assert!(app.security.capabilities.is_empty());
    }
}
