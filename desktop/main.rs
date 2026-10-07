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
        .on_page_load(move |window, _| {
            if test_mode {
                if break_ui {
                    window.eval("document.getElementById('app-ready').style.display = 'none'")
                        .expect("inject UI failure");
                }
                window.eval(include_str!("ui-test.js")).expect("start UI test");
            }
        })
        .run(tauri::generate_context!())
        .expect("launch DYApp");
}

#[cfg(test)]
mod tests {
    #[test]
    fn configured_window_is_visible_and_local() {
        let context = tauri::generate_context!();
        let windows = &context.config().tauri.windows;
        assert_eq!(windows.len(), 1);
        assert!(windows[0].visible);
        assert_eq!(windows[0].title, "DYApp");
        assert!(matches!(windows[0].url, tauri::WindowUrl::App(_)));
        assert!(!context.config().tauri.allowlist.all);
    }
}
