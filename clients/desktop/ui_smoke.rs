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
        include_str!("ui-test.js"),
    ]
    .join("\n")
}
