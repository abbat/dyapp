use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dyapp_identity::Identity;
use tauri::webview::PageLoadEvent;
use tauri::Manager;

#[path = "file_store.rs"]
mod file_store;
#[path = "system_store.rs"]
mod system_store;

struct IdentityState(Mutex<Option<Identity>>);

fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if std::env::args().any(|arg| arg == "--ui-test") {
        if let Some(path) = std::env::var_os("DYAPP_TEST_CONFIG_DIR") {
            return Ok(PathBuf::from(path));
        }
    }
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn read_mode(dir: &Path) -> Result<Option<String>, String> {
    let path = dir.join("identity-storage");
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut value = String::new();
    Read::by_ref(&mut file)
        .take(16)
        .read_to_string(&mut value)
        .map_err(|e| e.to_string())?;
    match value.as_str() {
        "system\n" | "file\n" => Ok(Some(value.trim_end().to_owned())),
        _ => Err("Unknown identity storage mode".into()),
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

fn write_mode(dir: &Path, mode: &str) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join("identity-storage"))
        .map_err(|e| e.to_string())?;
    file.write_all(format!("{mode}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn identity_state(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
) -> Result<HashMap<&'static str, String>, String> {
    let dir = config_dir(&app)?;
    let mut result = HashMap::new();
    match read_mode(&dir)? {
        Some(mode) => {
            let identity = tauri::async_runtime::spawn_blocking({
                let dir = dir.clone();
                let mode = mode.clone();
                move || match mode.as_str() {
                    "system" => system_store::load()?
                        .ok_or_else(|| "Identity key is missing in system storage".into()),
                    "file" => file_store::load(&dir.join("identity.key")),
                    _ => Err("Unknown identity storage mode".into()),
                }
            })
            .await
            .map_err(|e| e.to_string())??;
            result = ready_result(&mode, identity.peer_id(), &dir);
            *state.0.lock().map_err(|e| e.to_string())? = Some(identity);
        }
        None => {
            let availability = tauri::async_runtime::spawn_blocking(system_store::probe)
                .await
                .map_err(|e| e.to_string())?;
            result.insert("status", "setup".into());
            match availability {
                Ok(()) => {
                    result.insert("systemAvailable", "true".into());
                }
                Err(reason) => {
                    result.insert("systemAvailable", "false".into());
                    result.insert("systemReason", reason.key().into());
                }
            }
        }
    }
    Ok(result)
}

#[tauri::command]
async fn create_identity(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    mode: String,
) -> Result<HashMap<&'static str, String>, String> {
    if mode != "system" && mode != "file" {
        return Err("Unknown identity storage mode".into());
    }
    let dir = config_dir(&app)?;
    let (identity, mode, dir) = tauri::async_runtime::spawn_blocking(move || {
        if mode == "system" {
            system_store::probe()
                .map_err(|reason| format!("System storage unavailable: {:?}", reason))?;
        }
        write_mode(&dir, &mode)?;
        let identity = Identity::generate();
        match mode.as_str() {
            "system" => system_store::save(&identity)?,
            "file" => file_store::save(&dir.join("identity.key"), &identity)?,
            _ => unreachable!(),
        }
        Ok::<_, String>((identity, mode, dir))
    })
    .await
    .map_err(|e| e.to_string())??;
    let response = ready_result(&mode, identity.peer_id(), &dir);
    *state.0.lock().map_err(|e| e.to_string())? = Some(identity);
    Ok(response)
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

#[tauri::command]
fn ui_test_result(app: tauri::AppHandle, window: tauri::Window, passed: bool, peer_id: String) {
    if !std::env::args().any(|arg| arg == "--ui-test") {
        return;
    }
    let success = passed && window.is_visible().unwrap_or(false);
    if success {
        println!("UI_SMOKE_PASS:{peer_id}");
    } else {
        println!("UI_SMOKE_FAIL");
    }
    app.exit(if success { 0 } else { 1 });
}

fn main() {
    let test_mode = std::env::args().any(|arg| arg == "--ui-test");
    let break_ui = std::env::args().any(|arg| arg == "--break-ui");
    tauri::Builder::default()
        .manage(IdentityState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            identity_state,
            create_identity,
            pseudo_locale,
            ui_test_russian,
            ui_test_mode,
            ui_test_existing,
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
        assert_eq!(file["keyPath"], dir.join("identity.key").display().to_string());
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
