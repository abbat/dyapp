use tauri::utils::assets::{AssetKey, Assets};

#[test]
fn frontend_is_bundled_and_contains_ready_screen() {
    let context = tauri::generate_context!();
    let assets = context.assets();
    let index = assets.get(&AssetKey::from("index.html")).expect("bundled frontend");
    let html = std::str::from_utf8(&index).expect("UTF-8 frontend");
    assert!(html.contains("id=\"app-ready\""));
    assert!(html.contains("DYApp"));
    assert!(!html.contains("http://localhost"));
}
