//! Native smoke window synchronization. Signals follow completed OS operations/events.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tokio::sync::watch;

use crate::window::Rect;

#[derive(Clone, Copy, Default)]
struct Snapshot {
    visible: Option<bool>,
    focused: Option<bool>,
    bounds: Option<Rect>,
}

pub(crate) struct Signals(watch::Sender<Snapshot>);

impl Default for Signals {
    fn default() -> Self {
        Self(watch::channel(Snapshot::default()).0)
    }
}

fn bounds(window: &tauri::Window) -> Result<Rect, String> {
    let at = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    Ok(Rect {
        x: at.x,
        y: at.y,
        width: size.width,
        height: size.height,
    })
}

pub(crate) fn publish(window: &tauri::Window) {
    let (visible, focused) = (window.is_visible().ok(), window.is_focused().ok());
    if let Some(signals) = window.try_state::<Signals>() {
        signals.0.send_modify(|state| {
            state.visible = visible;
            state.focused = focused;
        });
    }
}

pub(crate) fn geometry(window: &tauri::Window) {
    if let Some(signals) = window.try_state::<Signals>() {
        signals.0.send_replace(Snapshot {
            visible: window.is_visible().ok(),
            focused: window.is_focused().ok(),
            bounds: bounds(window).ok(),
        });
    }
}

async fn wait(
    changes: &mut watch::Receiver<Snapshot>,
    ready: impl FnMut(&Snapshot) -> bool + Send,
    phase: &'static str,
) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(20), changes.wait_for(ready))
        .await
        .map_err(|_| format!("window readiness timed out: {phase}"))?
        .map(drop)
        .map_err(|error| error.to_string())
}

/// Hide/close, report native hidden readiness, then wait for an actual restore callback.
/// `restore: false` returns once hidden and unfocused, the state in which notices are shown;
/// `ui_test_restored` then waits for the restore.
#[tauri::command]
pub(crate) async fn ui_test_hide(
    app: tauri::AppHandle,
    window: tauri::Window,
    close: bool,
    restore: Option<bool>,
) -> Result<(), String> {
    ensure_test()?;
    let mut changes = window.state::<Signals>().0.subscribe();
    if close { window.close() } else { window.hide() }.map_err(|error| error.to_string())?;
    publish(&window);
    wait(&mut changes, |state| state.visible == Some(false), "hide").await?;
    marker(&app, "hidden")?;
    if restore == Some(false) {
        return wait(
            &mut changes,
            |state| state.focused == Some(false),
            "unfocus",
        )
        .await;
    }
    // A new deadline starts after hiding completed, rather than sharing a startup timer.
    wait(&mut changes, |state| state.visible == Some(true), "restore").await
}

/// Waits until the window is visible; the watch keeps the last state, so an earlier restore counts.
#[tauri::command]
pub(crate) async fn ui_test_restored(window: tauri::Window) -> Result<(), String> {
    ensure_test()?;
    let mut changes = window.state::<Signals>().0.subscribe();
    wait(&mut changes, |state| state.visible == Some(true), "restore").await
}

#[derive(Serialize, Deserialize)]
struct Expected {
    normal: Rect,
    displays: Vec<Rect>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Environment {
    min_size: [u32; 2],
    expected: Option<Rect>,
}

pub(crate) fn marker(app: &tauri::AppHandle, name: &str) -> Result<(), String> {
    ensure_test()?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("invalid readiness marker".into());
    }
    let root = crate::config_dir(app)?;
    let temp = root.join(format!("{name}.tmp"));
    std::fs::write(&temp, std::process::id().to_string())
        .and_then(|()| std::fs::rename(temp, root.join(name)))
        .map_err(|error| error.to_string())
}

fn ensure_test() -> Result<(), String> {
    if std::env::args().any(|arg| arg == "--ui-test")
        && std::env::var_os("DYAPP_TEST_CONFIG_DIR").is_some()
    {
        Ok(())
    } else {
        Err("UI test only".into())
    }
}

fn display(monitor: &tauri::Monitor) -> Rect {
    Rect {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    }
}

fn expected(window: &tauri::Window) -> Result<Option<Expected>, String> {
    let path = crate::config_dir(window.app_handle())?.join("expected-window.json");
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| error.to_string()),
    }
}

fn environment(window: &tauri::Window) -> Result<Environment, String> {
    let config = window.app_handle().config();
    let configured = config
        .app
        .windows
        .iter()
        .find(|item| item.label == window.label())
        .ok_or("no window configuration")?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let minimum: tauri::PhysicalSize<u32> = tauri::LogicalSize::new(
        configured.min_width.unwrap_or(1.0),
        configured.min_height.unwrap_or(1.0),
    )
    .to_physical(scale);
    Ok(Environment {
        min_size: [minimum.width, minimum.height],
        expected: expected(window)?.map(|value| value.normal),
    })
}

async fn resize(window: &tauri::Window, at: Rect) -> Result<(), String> {
    let mut changes = window.state::<Signals>().0.subscribe();
    geometry(window);
    window
        .set_size(tauri::PhysicalSize::new(at.width, at.height))
        .map_err(|error| error.to_string())?;
    wait(
        &mut changes,
        |state| {
            state
                .bounds
                .is_some_and(|value| value.width == at.width && value.height == at.height)
        },
        "resize",
    )
    .await?;
    window
        .set_position(tauri::PhysicalPosition::new(at.x, at.y))
        .map_err(|error| error.to_string())?;
    wait(
        &mut changes,
        |state| {
            state
                .bounds
                .is_some_and(|value| value.x == at.x && value.y == at.y)
        },
        "move",
    )
    .await
}

/// Bounds are physical pixels; constraints come from the actual configuration and display scale.
#[tauri::command]
pub(crate) async fn ui_test_window(
    window: tauri::Window,
    set: Option<Rect>,
    remember: Option<bool>,
) -> Result<(Rect, Rect, Environment), String> {
    ensure_test()?;
    if let Some(at) = set {
        resize(&window, at).await?;
        if remember == Some(true) {
            let displays = window
                .available_monitors()
                .map_err(|error| error.to_string())?
                .iter()
                .map(display)
                .collect();
            let fixture = Expected {
                normal: at,
                displays,
            };
            let path = crate::config_dir(window.app_handle())?.join("expected-window.json");
            std::fs::write(
                path,
                serde_json::to_vec(&fixture).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
    }
    let primary = window
        .primary_monitor()
        .map_err(|error| error.to_string())?
        .ok_or("no primary display")?;
    Ok((bounds(&window)?, display(&primary), environment(&window)?))
}
