use tauri::webview::PageLoadEvent;

#[tauri::command]
fn ui_test_result(app: tauri::AppHandle, window: tauri::Window, passed: bool) {
    if !std::env::args().any(|arg| arg == "--ui-test") {
        return;
    }
    let success = passed && window.is_visible().unwrap_or(false);
    println!("UI_SMOKE_{}", if success { "PASS" } else { "FAIL" });
    app.exit(if success { 0 } else { 1 });
}

fn main() {
    let test_mode = std::env::args().any(|arg| arg == "--ui-test");
    let break_ui = std::env::args().any(|arg| arg == "--break-ui");
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ui_test_result])
        .on_page_load(move |webview, payload| {
            if test_mode && payload.event() == PageLoadEvent::Finished {
                if break_ui {
                    webview
                        .eval("document.getElementById('app-ready').style.display = 'none'")
                        .expect("inject UI failure");
                }
                webview.eval(include_str!("ui-test.js")).expect("start UI test");
            }
        })
        .run(tauri::generate_context!())
        .expect("launch DYApp");
}

#[cfg(test)]
mod tests {
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
