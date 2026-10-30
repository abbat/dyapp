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
// The node library is Unix-only, so Windows builds without it (ADR 0030).
#[cfg(unix)]
#[path = "node.rs"]
mod node;
#[path = "profile.rs"]
mod profile;
#[path = "search.rs"]
mod search;
#[path = "slots.rs"]
mod slots;
#[path = "system_store.rs"]
mod system_store;
#[path = "tray.rs"]
mod tray;
#[path = "usage.rs"]
mod usage;
#[path = "window.rs"]
mod window;

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
    // The slot's node must not hold its folder open while it is removed. A cfg on a command
    // argument does not reach Tauri's macro, so the state comes from the app handle.
    #[cfg(unix)]
    node::stop(&app.state::<node::NodeState>()).await;
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

/// Connects to the active network as the app opens; `true` when a node of it answered.
#[tauri::command]
async fn connect_network(
    state: tauri::State<'_, IdentityState>,
    wait_ms: u32,
) -> Result<bool, String> {
    let dir = identity_dir(&state)?;
    network::connect(&dir, Duration::from_millis(wait_ms.into())).await
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

/// Countries with places for the place picker; the page names them.
#[tauri::command]
fn place_countries() -> Vec<&'static str> {
    dyapp_client::places::countries()
}

/// The place `id` of `country` in `lang`; see `dyapp_client::places::place`.
#[tauri::command]
fn place_name(country: String, id: u32, lang: String) -> Option<profile::Place> {
    dyapp_client::places::place(&country, id, &lang).map(Into::into)
}

/// Places of `country` whose name starts with `prefix`; see `dyapp_client::places::search`.
#[tauri::command]
fn search_places(
    country: String,
    prefix: String,
    lang: String,
    limit: usize,
) -> Vec<profile::Place> {
    let found = dyapp_client::places::search(&country, &prefix, &lang, limit);
    found.into_iter().map(Into::into).collect()
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

/// Hides the published profile from search with a tombstone; see `profile::hide`.
#[tauri::command]
async fn hide_profile(state: tauri::State<'_, IdentityState>) -> Result<profile::View, String> {
    let (identity, dir) = open_identity(&state)?;
    // The client store is not `Sync`, as in `publish_profile`.
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(profile::hide(&dir, &identity))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sends the held profile once its search window has ended; see `profile::send_search`.
#[tauri::command]
async fn send_search_profile(
    state: tauri::State<'_, IdentityState>,
) -> Result<profile::View, String> {
    let (identity, dir) = open_identity(&state)?;
    // The client store is not `Sync`, as in `publish_profile`.
    tauri::async_runtime::spawn_blocking(move || {
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
    let (identity, dir) = open_identity(&state)?;
    let cache = Arc::clone(&searching.0);
    // The client store is not `Sync`, so the search runs on a thread of its own.
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(search::search(&cache, &dir, &identity, query))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The open identity's node settings; see `node::load`.
#[cfg(unix)]
#[tauri::command]
async fn load_node_settings(
    state: tauri::State<'_, IdentityState>,
) -> Result<node::Settings, String> {
    node::load(&identity_dir(&state)?)
}

/// Saves the node settings and restarts the node as they say; see `node::apply`.
#[cfg(unix)]
#[tauri::command]
async fn save_node_settings(
    state: tauri::State<'_, IdentityState>,
    nodes: tauri::State<'_, node::NodeState>,
    settings: node::Settings,
) -> Result<HashMap<&'static str, String>, String> {
    let dir = identity_dir(&state)?;
    node::save(&dir, &settings)?;
    node::apply(&nodes, &dir).await
}

/// Starts or stops the node for the open identity, as on start or after a switch.
#[cfg(unix)]
#[tauri::command]
async fn apply_node(
    state: tauri::State<'_, IdentityState>,
    nodes: tauri::State<'_, node::NodeState>,
) -> Result<HashMap<&'static str, String>, String> {
    node::apply(&nodes, &identity_dir(&state)?).await
}

#[cfg(unix)]
#[tauri::command]
async fn node_status(
    state: tauri::State<'_, IdentityState>,
    nodes: tauri::State<'_, node::NodeState>,
) -> Result<HashMap<&'static str, String>, String> {
    Ok(node::status(&nodes, &identity_dir(&state)?).await)
}

/// Settings → Storage for the open identity; see `usage::usage`.
#[tauri::command]
async fn storage_usage(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
) -> Result<usage::Usage, String> {
    let config = config_dir(&app).ok();
    let dir = identity_dir(&state)?;
    tauri::async_runtime::spawn_blocking(move || usage::usage(config.as_deref(), &dir))
        .await
        .map_err(|e| e.to_string())
}

/// The status bar: client traffic since start and the open identity's message store bytes.
#[tauri::command]
fn status(state: tauri::State<'_, IdentityState>) -> usage::Status {
    usage::Status {
        traffic: dyapp_client::traffic(),
        messages: identity_dir(&state).ok().and_then(|dir| usage::messages(&dir)),
    }
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

/// UI test only, with `DYAPP_UI_SHOTS=<dir>`: saves the screen as `<dir>/<name>-<lang>-<width>.png`
/// at a narrow and a wide window, then restores the size. An existing file is kept, and a failed
/// screenshot never fails the test.
#[tauri::command]
async fn ui_test_shot(window: tauri::Window, name: String) -> Result<(), String> {
    let dir = match std::env::var_os("DYAPP_UI_SHOTS") {
        Some(dir) if std::env::args().any(|arg| arg == "--ui-test") => PathBuf::from(dir),
        _ => return Ok(()),
    };
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
        return Err("invalid screenshot name".into());
    }
    let lang = if ui_test_russian() { "ru" } else { "en" };
    let original = window.inner_size().map_err(|e| e.to_string())?;
    for width in [440u32, 1280] {
        let file = dir.join(format!("{name}-{lang}-{width}.png"));
        if file.exists() {
            continue;
        }
        let _ = window.set_position(tauri::LogicalPosition::new(0, 0));
        let _ = window.set_size(tauri::LogicalSize::new(width, 1400));
        // Xvfb has no window manager: give the webview time to lay out at the new size.
        std::thread::sleep(Duration::from_millis(600));
        let crop = format!("{width}x1400+0+0");
        let shot = std::process::Command::new("import")
            .args(["-window", "root", "-crop", &crop, "+repage"])
            .arg(&file)
            .status();
        if !shot.map(|s| s.success()).unwrap_or(false) {
            eprintln!("screenshot {} failed", file.display());
        }
    }
    let _ = window.set_size(original);
    Ok(())
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

/// Ctrl+Q: ends the process; the window state is saved on the way out.
#[tauri::command]
fn quit(app: tauri::AppHandle) {
    // The UI smoke ends its window steps here, so this exit is its pass.
    if std::env::args().any(|arg| arg == "--ui-test") {
        println!("UI_SMOKE_PASS:");
    }
    app.exit(0);
}

/// Settings → Window: the tray icon and close-to-tray, stored when `prefs` is given; `labels` are
/// Open and Quit in the UI language. Async: building the icon must not hold the main thread.
#[tauri::command]
async fn set_tray(
    app: tauri::AppHandle,
    prefs: Option<tray::Prefs>,
    labels: [String; 2],
) -> tray::State {
    tray::set(&app, config_dir(&app).ok().as_deref(), prefs, labels)
}

/// UI test only: closes the window (to the tray) or hides it, expects it off screen, writes `hidden`
/// to the test config dir for the smoke (a fake tray host or a second launch), then waits for the
/// window to come back.
#[tauri::command]
async fn ui_test_hide(
    app: tauri::AppHandle,
    window: tauri::Window,
    close: bool,
) -> Result<(), String> {
    if !std::env::args().any(|arg| arg == "--ui-test") {
        return Err("UI test only".into());
    }
    if close { window.close() } else { window.hide() }.map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(600));
    if window.is_visible().map_err(|e| e.to_string())? {
        return Err("the window stayed on screen".into());
    }
    std::fs::write(config_dir(&app)?.join("hidden"), "").map_err(|e| e.to_string())?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(500));
        if window.is_visible().map_err(|e| e.to_string())? {
            return Ok(());
        }
    }
    Err("the window did not come back".into())
}

/// UI test only: moves and resizes the window when `set` is given, then returns its bounds and
/// the primary display's, in physical pixels.
#[tauri::command]
async fn ui_test_window(
    window: tauri::Window,
    set: Option<window::Rect>,
) -> Result<[window::Rect; 2], String> {
    if !std::env::args().any(|arg| arg == "--ui-test") {
        return Err("UI test only".into());
    }
    if let Some(at) = set {
        window
            .set_size(tauri::PhysicalSize::new(at.width, at.height))
            .map_err(|e| e.to_string())?;
        window
            .set_position(tauri::PhysicalPosition::new(at.x, at.y))
            .map_err(|e| e.to_string())?;
        // Xvfb has no window manager: give the move time to land.
        std::thread::sleep(Duration::from_millis(600));
    }
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let monitor = window
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no display")?;
    let rect = |x, y, width, height| window::Rect { x, y, width, height };
    Ok([
        rect(position.x, position.y, size.width, size.height),
        rect(
            monitor.position().x,
            monitor.position().y,
            monitor.size().width,
            monitor.size().height,
        ),
    ])
}

fn main() {
    let test_mode = std::env::args().any(|arg| arg == "--ui-test");
    // The isolated loopback smoke uses a cheap node-ID puzzle on both ends.
    if test_mode && std::env::var_os("DYAPP_TEST_CONFIG_DIR").is_some() {
        dyapp_p2p_net::set_id_pow_bits(8);
    }
    let break_ui = std::env::args().any(|arg| arg == "--break-ui");
    // A second launch shows the running window and exits.
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show(app)));
    #[cfg(unix)]
    let builder = builder.manage(node::NodeState::default());
    builder
        .manage(IdentityState(Mutex::new(None)))
        .manage(NetworkState::default())
        .manage(ProfileState::default())
        .manage(search::SearchState::default())
        .manage(window::WindowState::default())
        .manage(tray::Tray::default())
        .setup(|app| {
            if let Some(main) = app.get_webview_window("main") {
                window::restore(&main, config_dir(app.handle()).ok().as_deref());
            }
            tray::watch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                window::track(window, &window.state::<window::WindowState>());
            }
            tauri::WindowEvent::CloseRequested { api, .. }
                if window.state::<tray::Tray>().hides_on_close() =>
            {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
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
            connect_network,
            profile_scales,
            place_countries,
            place_name,
            search_places,
            load_profile,
            save_profile,
            solve_profile,
            cancel_profile_solve,
            publish_profile,
            hide_profile,
            send_search_profile,
            search_profiles,
            #[cfg(unix)]
            load_node_settings,
            #[cfg(unix)]
            save_node_settings,
            #[cfg(unix)]
            apply_node,
            #[cfg(unix)]
            node_status,
            storage_usage,
            status,
            pseudo_locale,
            ui_test_russian,
            ui_test_mode,
            ui_test_existing,
            ui_test_step,
            ui_test_shot,
            ui_test_window,
            ui_test_hide,
            ui_test_result,
            set_tray,
            quit
        ])
        .on_page_load(move |webview, payload| {
            if test_mode && payload.event() == PageLoadEvent::Finished {
                if break_ui {
                    webview
                        .eval("document.querySelector('#profile-area h1').style.display = 'none'")
                        .expect("inject UI failure");
                }
                webview
                    .eval(include_str!("ui-test.js"))
                    .expect("start UI test");
            }
        })
        .build(tauri::generate_context!())
        .expect("launch DYApp")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Ok(config) = config_dir(app) {
                    window::save(&app.state::<window::WindowState>(), &config);
                }
            }
        });
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
        // Hidden until `window::restore` has placed it, then shown.
        assert!(!app.windows[0].visible);
        assert_eq!((app.windows[0].min_width, app.windows[0].min_height), (Some(400.0), Some(560.0)));
        assert_eq!(app.windows[0].title, "DYApp");
        assert!(matches!(app.windows[0].url, tauri::WebviewUrl::App(_)));
        // No capabilities: the page can call only the app's own command, no plugin APIs.
        assert!(app.security.capabilities.is_empty());
    }
}
