//! Shared native smoke script order, kept separate from application command registration.
use tauri::webview::PageLoadEvent;

const SCRIPTS: &[&str] = &[
    include_str!("interface.js"),
    include_str!("common.js"),
    include_str!("../video-frame-renderer.js"),
    include_str!("../video-frame-smoke.js"),
    include_str!("identity-peer-smoke.js"),
    include_str!("identity-profile-smoke.js"),
    include_str!("identity.js"),
    include_str!("chat-list-smoke.js"),
    include_str!("chat-message-smoke.js"),
    include_str!("chats.js"),
    include_str!("presence.js"),
    include_str!("contact-names.js"),
    include_str!("names-public-smoke.js"),
    include_str!("names-actions-smoke.js"),
    include_str!("names.js"),
    include_str!("profile-photos.js"),
    include_str!("peer-photos.js"),
    include_str!("chat-photo-fixture.js"),
    include_str!("photo-settings.js"),
    include_str!("chat-photo-send.js"),
    include_str!("chat-photo-render.js"),
    include_str!("photos.js"),
    include_str!("node-guard.js"),
    include_str!("network-guards.js"),
    include_str!("copy-guard.js"),
    include_str!("settings-guards.js"),
    include_str!("lifecycle-guards.js"),
    include_str!("guards.js"),
    include_str!("stickers.js"),
    include_str!("composer.js"),
    include_str!("sticker-picker.js"),
    include_str!("sticker-messages.js"),
    include_str!("chat-redraw.js"),
    include_str!("sticker-requests.js"),
    include_str!("sticker-file.js"),
    include_str!("sticker-peers.js"),
    include_str!("sharing.js"),
    include_str!("clear.js"),
    include_str!("identifiers.js"),
    include_str!("settings-nav.js"),
    include_str!("settings-lists.js"),
    include_str!("media-cache.js"),
    include_str!("sticker-removal.js"),
    include_str!("removal.js"),
    include_str!("system-steps.js"),
    include_str!("ready-step.js"),
    include_str!("add-step.js"),
    include_str!("main.js"),
];

pub(crate) fn scripts() -> String {
    let body = SCRIPTS.join("\n");
    format!(
        "(async () => {{\n{body}\n}})().catch(error => window.__TAURI__.core.invoke('ui_test_result', {{ passed: false, peerId: '', reason: String(error) }}));"
    )
}

/// Enable the isolated native smoke runner before the application is built.
pub(crate) fn attach(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    let test_mode = std::env::args().any(|arg| arg == "--ui-test");
    // The isolated loopback smoke uses a cheap node ID and profile proof on both ends.
    if test_mode && std::env::var_os("DYAPP_TEST_CONFIG_DIR").is_some() {
        dyapp_p2p_net::set_id_pow_bits(8);
        dyapp_profile::set_required_pow_bits(4);
    }
    let break_ui = std::env::args().any(|arg| arg == "--break-ui");
    // axe-core for the UI test's accessibility audit; a set but unreadable path fails the run.
    let axe = std::env::var_os("DYAPP_AXE")
        .filter(|_| test_mode)
        .map(|path| {
            std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("axe-core {}: {e}", path.to_string_lossy()))
        });
    builder.on_page_load(move |webview, payload| {
        if test_mode && payload.event() == PageLoadEvent::Finished {
            if break_ui {
                webview
                    .eval("document.querySelector('#profile-area h1').style.display = 'none'")
                    .expect("inject UI failure");
            }
            if let Some(axe) = &axe {
                webview.eval(axe.as_str()).expect("load axe-core");
            }
            webview.eval(scripts()).expect("start UI test");
        }
    })
}
