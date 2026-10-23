use tauri::utils::assets::AssetKey;

#[test]
fn frontend_is_bundled_and_contains_ready_screen() {
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    let assets = context.assets();
    let index = assets
        .get(&AssetKey::from("index.html"))
        .expect("bundled frontend");
    let html = std::str::from_utf8(&index).expect("UTF-8 frontend");
    assert!(html.contains("id=\"app-ready\""));
    assert!(html.contains("data-i18n=\"status.loading\""));
    assert!(html.contains("id=\"system-choice\""));
    assert!(html.contains("id=\"file-choice\""));
    assert!(!html.contains("http://localhost"));
    // Practice D1: the visible strings come from the bundled catalogue.
    let strings = assets
        .get(&AssetKey::from("i18n/en.json"))
        .expect("bundled catalogue");
    let strings = std::str::from_utf8(&strings).expect("UTF-8 catalogue");
    assert!(strings.contains("\"app.name\": \"DYApp\""));
    assert!(strings.contains("\"status.ready\": \"Ready\""));
}
