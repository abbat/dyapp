//! Shared native smoke script order, kept separate from application command registration.
pub(crate) fn scripts() -> String {
    [
        include_str!("ui-test-common.js"),
        include_str!("video-frame-renderer.js"),
        include_str!("video-frame-smoke.js"),
        include_str!("ui-test-identity.js"),
        include_str!("ui-test-chats.js"),
        include_str!("ui-test-presence.js"),
        include_str!("ui-test-names.js"),
        include_str!("ui-test-photos.js"),
        include_str!("ui-test-guards.js"),
        include_str!("ui-test-stickers.js"),
        include_str!("ui-test-composer.js"),
        include_str!("ui-test-sticker-picker.js"),
        include_str!("ui-test-sticker-messages.js"),
        include_str!("ui-test-chat-redraw.js"),
        include_str!("ui-test-clear.js"),
        include_str!("ui-test-identifiers.js"),
        include_str!("ui-test-settings-nav.js"),
        include_str!("ui-test-settings-lists.js"),
        include_str!("ui-test-media-cache.js"),
        include_str!("ui-test-sticker-removal.js"),
        include_str!("ui-test.js"),
    ]
    .join("\n")
}
