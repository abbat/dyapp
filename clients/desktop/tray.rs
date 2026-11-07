//! The tray icon with Open and Quit, and the Settings that control it (v0.0.1-client-screens.md,
//! Desktop window and tray). On Linux a tray exists only while a StatusNotifierItem host is on the
//! session bus; when it goes away the window is shown again, so the app never stays unreachable.

/// Ctrl+Q: ends the process; the window state is saved on the way out.
#[tauri::command]
pub(crate) fn quit(app: tauri::AppHandle) {
    // The UI smoke ends its window steps here, so this exit is its pass.
    if std::env::args().any(|arg| arg == "--ui-test") {
        println!("UI_SMOKE_PASS:");
    }
    app.exit(0);
}

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    pub show: bool,
    pub close_to_tray: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            show: true,
            close_to_tray: false,
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub available: bool,
    pub show: bool,
    pub close_to_tray: bool,
}

#[derive(Default)]
struct Inner {
    prefs: Option<Prefs>,
    available: bool,
    /// Open, Quit, fixed notice and Sticker in the UI language; the icon waits for them.
    labels: Option<[String; 4]>,
    shown: bool,
}

/// `hides` mirrors the state for window events: the main thread must not wait on the lock while
/// the watcher holds it and waits on the main thread to build the icon.
#[derive(Default)]
pub struct Tray {
    inner: Mutex<Inner>,
    hides: AtomicBool,
}

const FILE: &str = "tray.json";
const ID: &str = "main";
#[cfg(unix)]
const WATCHER: &str = "org.kde.StatusNotifierWatcher";

impl Tray {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Closing the window hides it only while the icon is there to bring it back.
    pub fn hides_on_close(&self) -> bool {
        self.hides.load(Ordering::SeqCst)
    }
}

/// Settings → Window: loads the saved choice on first call, stores `prefs` when given, takes the
/// menu labels, then shows or removes the icon to match.
pub fn set(
    app: &AppHandle,
    config: Option<&Path>,
    prefs: Option<Prefs>,
    labels: [String; 4],
) -> State {
    let tray = app.state::<Tray>();
    let mut inner = tray.lock();
    inner.prefs.get_or_insert_with(|| {
        config
            .and_then(|dir| fs::read(dir.join(FILE)).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    });
    if let Some(prefs) = prefs {
        // Without the icon there is nothing to close to.
        let prefs = Prefs {
            close_to_tray: prefs.show && prefs.close_to_tray,
            ..prefs
        };
        if let (Some(dir), Ok(json)) = (config, serde_json::to_vec(&prefs)) {
            let _ = fs::create_dir_all(dir);
            let _ = fs::write(dir.join(FILE), json);
        }
        inner.prefs = Some(prefs);
    }
    let rebuild = inner.labels.as_ref() != Some(&labels);
    inner.labels = Some(labels);
    apply(app, &tray, &mut inner, rebuild);
    let prefs = inner.prefs.unwrap_or_default();
    State {
        available: inner.available,
        show: prefs.show,
        close_to_tray: prefs.close_to_tray,
    }
}

/// Shows the icon when the host, the labels and the setting allow it, and removes it otherwise.
fn apply(app: &AppHandle, tray: &Tray, inner: &mut Inner, rebuild: bool) {
    let wanted = inner.available && inner.prefs.is_some_and(|prefs| prefs.show);
    let labels = inner.labels.clone().filter(|_| wanted);
    if inner.shown && (rebuild || labels.is_none()) {
        app.remove_tray_by_id(ID);
        inner.shown = false;
    }
    if let (Some([open, quit, _, _]), false) = (labels, inner.shown) {
        inner.shown = build(app, &open, &quit).is_ok();
    }
    let close = inner.prefs.is_some_and(|prefs| prefs.close_to_tray);
    tray.hides.store(inner.shown && close, Ordering::SeqCst);
}

fn build(app: &AppHandle, open: &str, quit: &str) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", open, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    let mut builder = TrayIconBuilder::with_id(ID)
        .tooltip("DYApp")
        .menu(&Menu::with_items(app, &[&open, &quit])?)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "quit" => self::quit(app.clone()),
            _ => {}
        });
    // Without XDG_RUNTIME_DIR the default is /tmp/tray-icon, which another user may own.
    if let Ok(dir) = app.path().app_cache_dir() {
        builder = builder.temp_dir_path(dir.join("tray-icon"));
    }
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app).map(drop)
}

/// Open, fixed notice and Sticker labels, once the page has given them.
pub fn notice_labels(app: &AppHandle) -> Option<(String, String, String)> {
    let tray = app.state::<Tray>();
    let inner = tray.lock();
    let labels = inner.labels.as_ref();
    labels.map(|[open, _, notice, sticker]| (open.clone(), notice.clone(), sticker.clone()))
}

pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// A host appeared or went away; a window hidden to a vanished tray comes back.
fn available(app: &AppHandle, now: bool) {
    let tray = app.state::<Tray>();
    let mut inner = tray.lock();
    if inner.available == now {
        return;
    }
    inner.available = now;
    apply(app, &tray, &mut inner, false);
    drop(inner);
    if !now {
        show(app);
    }
}

/// The appindicator library the tray loads at run time; without it tray-icon panics.
#[cfg(unix)]
fn library() -> bool {
    std::process::Command::new("/sbin/ldconfig")
        .arg("-p")
        .output()
        .map(|out| {
            let text = String::from_utf8_lossy(&out.stdout);
            ["libayatana-appindicator3.so", "libappindicator3.so"]
                .iter()
                .any(|name| text.contains(name))
        })
        .unwrap_or(false)
}

/// Follows the StatusNotifierItem host on the session bus, checking every two seconds.
#[cfg(unix)]
pub fn watch(app: AppHandle) {
    if !library() {
        return;
    }
    std::thread::spawn(move || {
        tauri::async_runtime::block_on(async move {
            let Ok(bus) = zbus::Connection::session().await else {
                return;
            };
            let Ok(dbus) = zbus::fdo::DBusProxy::new(&bus).await else {
                return;
            };
            let Ok(name) = zbus::names::BusName::try_from(WATCHER) else {
                return;
            };
            loop {
                available(
                    &app,
                    dbus.name_has_owner(name.clone()).await.unwrap_or(false),
                );
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
    });
}

/// Windows always has a notification area.
#[cfg(unix)]
const INSTANCE: &str = "com.dyapp.SingleInstance";
#[cfg(unix)]
const INSTANCE_PATH: &str = "/com/dyapp/SingleInstance";

#[cfg(unix)]
struct Instance(AppHandle);

#[cfg(unix)]
#[zbus::interface(name = "com.dyapp.SingleInstance")]
impl Instance {
    fn show(&self) {
        show(&self.0);
    }
}

/// The first launch of a session owns a bus name; a later one has it show the window and exits.
/// Without a session bus every launch goes on. tauri-plugin-single-instance does the same on
/// zbus 5, whose retired crates stay out of this lockfile. A plugin hook, as the window must not
/// open first.
#[cfg(unix)]
pub fn one_instance() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("single-instance")
        .setup(|app, _| {
            tauri::async_runtime::block_on(async {
                let served = zbus::connection::Builder::session()
                    .serve_at(INSTANCE_PATH, Instance(app.clone()))
                    .build()
                    .await;
                let Ok(bus) = served else { return };
                // The default flags would take the name away from the first launch.
                let only = zbus::fdo::RequestNameFlags::DO_NOT_QUEUE;
                match bus.request_name_with_flags(INSTANCE, only).await {
                    Ok(_) => {
                        app.manage(bus);
                    }
                    Err(zbus::Error::NameTaken) => {
                        let path = Some(INSTANCE);
                        let _ = bus
                            .call_method(path, INSTANCE_PATH, path, "Show", &())
                            .await;
                        std::process::exit(0);
                    }
                    Err(_) => {}
                }
            });
            Ok(())
        })
        .build()
}

#[cfg(not(unix))]
pub fn one_instance() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_single_instance::init(|app, _, _| show(app))
}

#[cfg(not(unix))]
pub fn watch(app: AppHandle) {
    available(&app, true);
}
