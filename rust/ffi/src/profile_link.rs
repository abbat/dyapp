//! The shared profile link, so every client reads and writes one format.

/// The chat message text that shares a profile; none for a Peer ID that is not canonical.
#[uniffi::export]
pub fn profile_link_text(peer_id: String) -> Option<String> {
    dyapp_identity::profile_link::profile_link(&peer_id)
}

/// The Peer ID to open when a message body is exactly one profile link.
#[uniffi::export]
pub fn profile_link_peer(text: String) -> Option<String> {
    dyapp_identity::profile_link::parse_profile_link(&text).map(str::to_owned)
}
