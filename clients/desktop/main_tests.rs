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
    assert_eq!(
        (app.windows[0].min_width, app.windows[0].min_height),
        (Some(400.0), Some(560.0))
    );
    assert_eq!(app.windows[0].title, "DYApp");
    assert!(matches!(app.windows[0].url, tauri::WebviewUrl::App(_)));
    // No capabilities: the page can call only the app's own command, no plugin APIs.
    assert!(app.security.capabilities.is_empty());
}
