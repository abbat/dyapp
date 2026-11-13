use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dyapp_identity::Identity;
use network::{Draft, NetworkState};
use profile::ProfileState;
use slots::{Opened, Slot};
use tauri::Manager;

mod chat_photos;
#[path = "chats.rs"]
mod chats;
mod clipboard;
mod contact_notes;
mod favorites;
#[path = "file_store.rs"]
mod file_store;
#[path = "identity_gate.rs"]
mod identity_gate;
mod identity_runtime;
mod stickers;
#[cfg(feature = "ui-smoke")]
#[path = "ui-tests/mod.rs"]
mod ui_smoke;
use identity_runtime::{describe, respond, IdentityState};
#[path = "link.rs"]
mod link;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;
#[path = "network.rs"]
mod network;
// Installation-owned embedded node shared by desktop platforms (ADR 0030).
#[path = "node.rs"]
mod node;
mod node_usage_commands;
#[path = "notify.rs"]
mod notify;
#[path = "peer_photos.rs"]
mod peer_photos;
mod photo_prepare;
mod places;
#[path = "privacy.rs"]
mod privacy;
#[path = "profile.rs"]
mod profile;
#[path = "profile_photos.rs"]
mod profile_photos;
mod safety;
#[path = "sealing.rs"]
mod sealing;
#[path = "search.rs"]
mod search;
mod slot_keys;
#[path = "slots.rs"]
mod slots;
use sealing::sealing_key;
#[path = "system_store.rs"]
mod system_store;
#[path = "tray.rs"]
mod tray;
#[path = "usage.rs"]
mod usage;
#[cfg(all(feature = "ui-smoke", target_os = "linux", debug_assertions))]
mod video_frames;
#[path = "window.rs"]
mod window;

fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(feature = "ui-smoke")]
    if std::env::args().any(|arg| arg == "--ui-test") {
        if let Some(path) = std::env::var_os("DYAPP_TEST_CONFIG_DIR") {
            return Ok(PathBuf::from(path));
        }
    }
    app.path().app_config_dir().map_err(|e| e.to_string())
}

// A key of `slot`'s storage mode: the file at `path` or the system slot `name`.
fn load_key(slot: &Slot, path: &Path, name: &str) -> Result<Option<Identity>, String> {
    match slot.mode {
        Some("system") => system_store::load_slot(name).inspect_err(|error| {
            eprintln!("Secret Service identity load failed: {error}");
        }),
        Some(_) => match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            _ => file_store::load(path).map(Some),
        },
        None => Err("Unknown identity storage mode".into()),
    }
}

fn save_key(slot: &Slot, path: &Path, name: &str, key: &Identity) -> Result<(), String> {
    match slot.mode {
        Some("system") => system_store::save_slot(name, key),
        Some(_) => file_store::save(path, key),
        None => Err("Unknown identity storage mode".into()),
    }
}

fn erase_key(slot: &Slot, path: &Path, name: &str) -> Result<(), String> {
    match slot.mode {
        Some("system") => system_store::delete_slot(name),
        Some(_) => match fs::remove_file(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result.map_err(|e| e.to_string()),
        },
        None => Err("Unknown identity storage mode".into()),
    }
}

fn load(slot: &Slot) -> Result<Option<Identity>, String> {
    load_key(slot, &slot.key_path(), &slot.system_name())
}

fn save(slot: &Slot, identity: &Identity) -> Result<(), String> {
    save_key(slot, &slot.key_path(), &slot.system_name(), identity)
}

// Device secrets go first, so a failure leaves the identity key for a retry to confirm.
fn erase(slot: &Slot) -> Result<(), String> {
    erase_key(slot, &slot.seal_path(), &slot.seal_name())?;
    erase_key(slot, &slot.device_path(), &slot.device_name())?;
    erase_key(slot, &slot.key_path(), &slot.system_name())
}

/// The open identity's device mailbox key, made on first use; see `slots::device_key`.
fn device_key(root: &Path, dir: &Path, identity: &Identity) -> Result<Identity, String> {
    let slot = slots::list(root)?
        .into_iter()
        .find(|slot| slot.dir == dir && slot.state == slots::State::Ready)
        .ok_or("Unknown identity")?;
    let database = dir.join("client.db");
    if load_key(&slot, &slot.device_path(), &slot.device_name())?.is_none()
        && database.exists()
        && dyapp_client::store::Store::open(&database, identity)
            .map_err(|e| e.to_string())?
            .sealed_device()
            .map_err(|e| e.to_string())?
            .is_some()
    {
        return Err("Device key is missing; MLS state cannot be reset".into());
    }
    slots::device_key(
        root,
        dir,
        &|slot: &Slot| load_key(slot, &slot.device_path(), &slot.device_name()),
        &|slot: &Slot, key: &Identity| {
            save_key(slot, &slot.device_path(), &slot.device_name(), key)
        },
    )
}

/// Stop admitted long-running work after closing admission, then wait off the UI thread.
async fn drain_identity(
    app: &tauri::AppHandle,
    state: &IdentityState,
) -> Result<identity_gate::Transition<identity_runtime::ActiveIdentity>, String> {
    let transition = state.start_transition_with(|| {
        app.state::<ProfileState>().cancel();
        network::cancel(&app.state::<NetworkState>());
    })?;
    let searching = app.state::<search::SearchState>();
    let checks = app.state::<NetworkState>();
    drain_and_clear(transition, &searching, &checks).await
}

async fn drain_and_clear(
    transition: identity_gate::Transition<identity_runtime::ActiveIdentity>,
    searching: &search::SearchState,
    checks: &NetworkState,
) -> Result<identity_gate::Transition<identity_runtime::ActiveIdentity>, String> {
    let transition = IdentityState::drain(transition).await?;
    searching.clear()?;
    checks.clear_checked()?;
    Ok(transition)
}

#[tauri::command]
async fn identity_state(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
) -> Result<HashMap<&'static str, String>, String> {
    let transition = drain_identity(&app, &state).await?;
    let dir = config_dir(&app)?;
    let opened = tauri::async_runtime::spawn_blocking(move || slots::open(&dir, &load))
        .await
        .map_err(|e| e.to_string())??;
    let setup = matches!(opened, Opened::Setup);
    let result = respond(opened, transition)?;
    if setup {
        storage_options().await
    } else {
        Ok(result)
    }
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
    let transition = drain_identity(&app, &state).await?;
    let dir = config_dir(&app)?;
    let opened = tauri::async_runtime::spawn_blocking(move || {
        let slot = slots::list(&dir)?
            .into_iter()
            .find(|listed| listed.id == slot)
            .ok_or("Unknown identity")?;
        let id = slot.id.clone();
        let opened = slots::open_slot(slot, &load)?;
        slots::set_active(&dir, &id)?;
        Ok::<_, String>(opened)
    })
    .await
    .map_err(|e| e.to_string())??;
    respond(opened, transition)
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
    let transition = drain_identity(&app, &state).await?;
    let dir = config_dir(&app)?;
    let recover_dir = dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        if mode == "system" {
            system_store::probe()
                .map_err(|reason| format!("System storage unavailable: {reason:?}"))?;
        }
        slots::create(&dir, mode, &save, &load)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    match result {
        Ok((slot, identity)) => {
            app.state::<link::Links>().made(&slot.id);
            respond(Opened::Ready(slot, identity), transition)
        }
        Err(error) => {
            IdentityState::reconcile(recover_dir, transition).await?;
            Err(error)
        }
    }
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
    let transition = drain_identity(&app, &state).await?;
    let dir = config_dir(&app)?;
    let recover_dir = dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        slots::remove(&dir, &slot, peer_id.as_deref(), &load, &erase)?;
        slots::list(&dir).map(|slots| slots.len())
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    let remaining = match result {
        Ok(remaining) => remaining,
        Err(error) => {
            IdentityState::reconcile(recover_dir, transition).await?;
            return Err(error);
        }
    };
    transition.commit(None).map_err(|e| format!("{e:?}"))?;
    if remaining == 0 {
        return storage_options().await;
    }
    Ok(describe(Opened::Picker).0)
}

/// The open identity's `draft` or `active` bootstrap settings, `null` when never saved.
#[tauri::command]
async fn load_network(
    state: tauri::State<'_, IdentityState>,
    file: String,
) -> Result<Option<Draft>, String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    // A cancelled command cannot stop its blocking worker; the worker owns the lease.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        network::load(&dir, &file)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_network_draft(
    state: tauri::State<'_, IdentityState>,
    draft: Draft,
) -> Result<(), String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        network::save_draft(&dir, draft)
    })
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
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    network::check(
        &checks,
        dir,
        draft,
        Duration::from_millis(wait_ms.into()),
        || lease.transitioning().map_err(|error| format!("{error:?}")),
    )
    .await
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
fn cancel_connection_check(checks: tauri::State<'_, NetworkState>) {
    network::cancel(&checks);
}

/// Connects to the active network as the app opens; `true` when a node of it answered.
#[tauri::command]
async fn connect_network(
    state: tauri::State<'_, IdentityState>,
    wait_ms: u32,
) -> Result<bool, String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    network::connect(&dir, Duration::from_millis(wait_ms.into())).await
}

#[tauri::command]
async fn activate_connection(
    state: tauri::State<'_, IdentityState>,
    checks: tauri::State<'_, NetworkState>,
    searching: tauri::State<'_, search::SearchState>,
) -> Result<Draft, String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    searching.activate_network(&checks, &dir)
}

/// Picker values from the shared profile vocabulary.
#[tauri::command]
fn profile_scales() -> Vec<profile::PickerScale> {
    profile::scales()
}

/// The open identity's profile draft; see `profile::load`.
#[tauri::command]
async fn load_profile(state: tauri::State<'_, IdentityState>) -> Result<profile::View, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        profile::load(&dir, &identity)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_profile(
    state: tauri::State<'_, IdentityState>,
    draft: profile::Draft,
) -> Result<profile::View, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        profile::save(&dir, &identity, draft)
    })
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
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let cancel = solving.begin()?;
    // The transition may have signalled cancellation before this solve registered.
    if lease
        .transitioning()
        .map_err(|error| format!("{error:?}"))?
    {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let bits = dyapp_profile::required_pow_bits();
        profile::solve(&cancel, &dir, &identity, attested, bits)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
fn cancel_profile_solve(solving: tauri::State<'_, ProfileState>) {
    solving.cancel();
}

/// Publishes the solved draft and checks the readback; see `profile::publish`.
#[tauri::command]
async fn publish_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    attested: bool,
) -> Result<profile::View, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    // The client store is not `Sync`, so the publish runs on a thread of its own.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        let device = device_key(&root, &dir, &identity)?;
        tauri::async_runtime::block_on(profile::publish(
            &root,
            &dir,
            &identity,
            &device,
            attested,
            dyapp_profile::required_pow_bits(),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Hides the published profile from search with a tombstone; see `profile::hide`.
#[tauri::command]
async fn hide_profile(state: tauri::State<'_, IdentityState>) -> Result<profile::View, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    // The client store is not `Sync`, as in `publish_profile`.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(profile::hide(&dir, &identity))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Publishes final deletion to profile storage and search before removing an identity.
#[tauri::command]
async fn withdraw_profile(state: tauri::State<'_, IdentityState>) -> Result<profile::View, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(profile::withdraw(&dir, &identity))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sends the held profile once its search window has ended; see `profile::send_search`.
#[tauri::command]
async fn send_search_profile(
    state: tauri::State<'_, IdentityState>,
) -> Result<profile::View, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    // The client store is not `Sync`, as in `publish_profile`.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(profile::send_search(&dir, &identity))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Verified profile search on the active network; see `search::search`.
#[tauri::command]
async fn search_profiles(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, search::SearchState>,
    query: search::Query,
) -> Result<search::Results, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let cache = Arc::clone(&searching.0);
    // The client store is not `Sync`, so the search runs on a thread of its own.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(search::search(&cache, &dir, &identity, query))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Locale flags: pseudo-localization (`--pseudo-locale`), then the smoke-only Russian override.
#[tauri::command]
fn locale_options() -> [bool; 2] {
    let pseudo = std::env::args().any(|arg| arg == "--pseudo-locale");
    #[cfg(feature = "ui-smoke")]
    let russian = ui_test_russian();
    #[cfg(not(feature = "ui-smoke"))]
    let russian = false;
    [pseudo, russian]
}

#[cfg(feature = "ui-smoke")]
fn ui_test_russian() -> bool {
    std::env::args().any(|arg| arg == "--ui-test")
        && std::env::args().any(|arg| arg == "--ui-test-ru")
}

#[cfg(feature = "ui-smoke")]
#[tauri::command]
fn ui_test_mode() -> String {
    if std::env::args().any(|arg| arg == "--ui-test-file") {
        "file".into()
    } else {
        "system".into()
    }
}

#[cfg(feature = "ui-smoke")]
#[tauri::command]
fn ui_test_existing() -> bool {
    std::env::args().any(|arg| arg == "--ui-test-existing")
}

/// UI test flow: add, pick, recovery, remove, network, network-restart, or connect:<seed>.
#[cfg(feature = "ui-smoke")]
#[tauri::command]
fn ui_test_step() -> String {
    std::env::args()
        .find_map(|arg| arg.strip_prefix("--ui-test-step=").map(str::to_owned))
        .unwrap_or_default()
}

/// UI test screenshots at 440 and 1280 px wide, 1400 px high, when `DYAPP_UI_SHOTS=<dir>` is set.
/// Without `width` it returns the widths still missing; existing files are kept, empty ones fail.
/// With `width` it captures the top-left screen area: the caller has already placed the window
/// there and waited for the webview to paint at that size.
#[cfg(feature = "ui-smoke")]
#[tauri::command]
async fn ui_test_shot(
    window: tauri::Window,
    name: String,
    width: Option<u32>,
) -> Result<Vec<u32>, String> {
    if width.is_none() {
        // A trace of the screens reached, so a timed-out suite shows where it stopped.
        eprintln!("ui-test screen {name}");
    }
    let dir = match std::env::var_os("DYAPP_UI_SHOTS") {
        Some(dir) if std::env::args().any(|arg| arg == "--ui-test") => PathBuf::from(dir),
        _ => return Ok(Vec::new()),
    };
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
        return Err("invalid screenshot name".into());
    }
    let lang = if ui_test_russian() { "ru" } else { "en" };
    let file = |width: u32| dir.join(format!("{name}-{lang}-{width}.png"));
    let filled = |file: &std::path::Path| file.metadata().is_ok_and(|metadata| metadata.len() > 0);
    let Some(width) = width else {
        let mut missing = Vec::new();
        for width in [440u32, 1280] {
            let file = file(width);
            if !file.exists() {
                missing.push(width);
            } else if !filled(&file) {
                return Err(format!("screenshot {} is empty", file.display()));
            }
        }
        return Ok(missing);
    };
    if ![440, 1280].contains(&width) {
        return Err("invalid screenshot width".into());
    }
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size: tauri::PhysicalSize<u32> = tauri::LogicalSize::new(width, 1400).to_physical(scale);
    let crop = format!("{}x{}+0+0", size.width, size.height);
    let file = file(width);
    let shot = std::process::Command::new("import")
        .args(["-window", "root", "-crop", &crop, "+repage"])
        .arg(&file)
        .status();
    if shot.is_ok_and(|s| s.success()) && filled(&file) {
        Ok(Vec::new())
    } else {
        Err(format!("screenshot {} failed or is empty", file.display()))
    }
}

#[cfg(feature = "ui-smoke")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
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
        if reason.starts_with("video:") {
            println!(
                "UI_SMOKE_METRICS:{}",
                reason.chars().take(512).collect::<String>()
            );
        }
        println!("UI_SMOKE_PASS:{peer_id}");
    } else {
        // Keep bounded diagnostics on one line without losing axe's underlying check failure.
        let reason = reason.replace(['\r', '\n'], " ");
        println!(
            "UI_SMOKE_FAIL: {}",
            reason.chars().take(8192).collect::<String>()
        );
    }
    app.exit(i32::from(!success));
}

/// Settings → Window: the tray icon and close-to-tray, stored when `prefs` is given; `labels` are
/// Open and Quit in the UI language. Async: building the icon must not hold the main thread.
#[tauri::command]
async fn set_tray(
    app: tauri::AppHandle,
    prefs: Option<tray::Prefs>,
    labels: [String; 4],
) -> tray::State {
    tray::set(&app, config_dir(&app).ok().as_deref(), prefs, labels)
}

const COMMAND_HANDLER: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
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
    connect_network,
    profile_scales,
    places::place_countries,
    places::place_name,
    places::search_places,
    load_profile,
    save_profile,
    peer_photos::fetch_peer_photos,
    peer_photos::peer_photo,
    peer_photos::peer_photo_auto,
    photo_prepare::photo_picked_byte_limit,
    photo_prepare::photo_decode_plan,
    profile_photos::add_profile_photo,
    profile_photos::remove_profile_photo,
    profile_photos::move_profile_photo,
    profile_photos::profile_photo,
    solve_profile,
    cancel_profile_solve,
    publish_profile,
    hide_profile,
    withdraw_profile,
    send_search_profile,
    search_profiles,
    safety::confirm_contact,
    safety::contact_safety_number,
    safety::contact_safety_qr,
    safety::contact_verified,
    safety::unverify_contact,
    safety::contact_new_device,
    safety::acknowledge_new_device,
    safety::scan_contact_qr,
    clipboard::copy_safety_number,
    clipboard::clear_safety_clipboard,
    link::start_link,
    link::cancel_link,
    link::poll_link,
    link::link_pending,
    link::linked_device,
    #[cfg(feature = "ui-smoke")]
    link::ui_test_link_code,
    chats::own::check_link_code,
    chats::own::list_devices,
    chats::own::send_link,
    chats::own::revoke_device,
    chats::commands::block_peer,
    chats::commands::peer_likes,
    chats::commands::like_peer,
    chats::commands::liked_you,
    chats::share::shared_profile,
    chats::share::share_profile,
    favorites::favorite_peers,
    favorites::favorite_state,
    favorites::set_favorite,
    contact_notes::contact_note,
    contact_notes::set_contact_note,
    stickers::import_sticker_pack,
    stickers::saved_sticker_packs,
    stickers::remove_saved_sticker_pack,
    chats::commands::receive_mail,
    chats::commands::chat_composer,
    chats::commands::chat_rows,
    chats::commands::chat_history,
    stickers::chat::chat_sticker_frame,
    stickers::picker::saved_sticker_page,
    stickers::picker::recents::recent_sticker_page,
    stickers::send::send_saved_sticker,
    stickers::requests::chat_sticker_pack_status,
    stickers::requests::actions::change_sticker_pack_request,
    chat_photos::chat_photo_original,
    chat_photos::chat_photo_auto,
    chat_photos::auto_download_mode,
    chat_photos::set_auto_download_mode,
    chat_photos::kept_originals_mib,
    chat_photos::set_kept_originals_mib,
    chat_photos::cache::media_cache,
    chat_photos::cache::set_media_cache,
    chat_photos::cache::clear_media_cache,
    chats::commands::send_chat_text,
    chats::forward::forward_chat_text,
    chats::send_photo::send_chat_photo,
    chats::presence::announce_presence,
    chats::names::contact_names,
    chats::names::set_contact_name,
    chats::operations::edit_chat_text,
    chats::operations::delete_chat_text,
    chats::commands::clear_chat,
    chats::commands::mark_chat_read,
    chats::commands::send_read_receipts,
    privacy::commands::privacy_settings,
    privacy::commands::set_privacy,
    privacy::commands::blocked_peers,
    privacy::commands::unblock_peer,
    privacy::commands::set_muted,
    privacy::commands::passed_peers,
    privacy::commands::pass_peer,
    node_usage_commands::load_node_settings,
    node_usage_commands::save_node_settings,
    node_usage_commands::apply_node,
    node_usage_commands::node_status,
    node_usage_commands::storage_usage,
    node_usage_commands::status,
    locale_options,
    #[cfg(feature = "ui-smoke")]
    ui_test_mode,
    #[cfg(feature = "ui-smoke")]
    ui_test_existing,
    #[cfg(feature = "ui-smoke")]
    ui_test_step,
    #[cfg(feature = "ui-smoke")]
    ui_test_shot,
    #[cfg(feature = "ui-smoke")]
    ui_smoke::window::ui_test_window,
    #[cfg(feature = "ui-smoke")]
    ui_smoke::window::ui_test_hide,
    #[cfg(feature = "ui-smoke")]
    ui_smoke::window::ui_test_restored,
    #[cfg(feature = "ui-smoke")]
    ui_test_result,
    #[cfg(all(feature = "ui-smoke", target_os = "linux", debug_assertions))]
    video_frames::ui_test_video_frames,
    set_tray,
    tray::quit
];

fn main() {
    // A second launch shows the running window and exits.
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tray::one_instance());
    #[cfg(target_os = "linux")]
    let builder = builder.on_permission_request(safety::camera_permission);
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_plugin_notification::init());
    let builder = builder.manage(node::NodeState::default());
    let builder = builder
        .manage(IdentityState::new())
        .manage(NetworkState::default())
        .manage(ProfileState::default())
        .manage(search::SearchState::default())
        .manage(chats::Sessions::default())
        .manage(link::Links::default())
        .manage(window::WindowState::default())
        .manage(tray::Tray::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            macos::setup(app.handle())?;
            if let Some(main) = app.get_webview_window("main") {
                window::restore(&main, config_dir(app.handle()).ok().as_deref());
            }
            node_usage_commands::start(app.handle().clone());
            tray::watch(app.handle().clone());
            notify::watch(app.handle().clone());
            chats::commands::watch_mail(app.handle().clone());
            Ok(())
        })
        .on_window_event(window::event)
        .invoke_handler(COMMAND_HANDLER);
    #[cfg(feature = "ui-smoke")]
    let builder = ui_smoke::attach(builder);
    builder
        .build(tauri::generate_context!())
        .expect("launch DYApp")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                #[cfg(feature = "ui-smoke")]
                let _ = ui_smoke::window::marker(app, "dock-reopened");
                tray::show(app);
            }
            if let tauri::RunEvent::Exit = event {
                node_usage_commands::shutdown(app);
                if let Ok(config) = config_dir(app) {
                    window::save(&app.state::<window::WindowState>(), &config);
                }
            }
        });
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
