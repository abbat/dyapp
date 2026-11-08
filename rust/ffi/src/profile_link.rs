//! The shared profile link, so every client reads and writes one format.

/// The chat message text that shares a profile; none for a Peer ID that is not canonical.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn profile_link_text(peer_id: String) -> Option<String> {
    dyapp_identity::profile_link::profile_link(&peer_id)
}

/// The Peer ID to open when a message body is exactly one profile link.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn profile_link_peer(text: String) -> Option<String> {
    dyapp_identity::profile_link::parse_profile_link(&text).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn the_exports_keep_the_core_format_and_refusals() {
        let text = profile_link_text(PEER.to_owned()).unwrap();
        assert_eq!(text, format!("dyapp-profile:1:{PEER}"));
        assert_eq!(profile_link_peer(text).as_deref(), Some(PEER));
        assert_eq!(profile_link_text("not a peer".to_owned()), None);
        assert_eq!(profile_link_peer(PEER.to_owned()), None);
    }
}
