//! Contact safety-number command for a verified profile shown by the desktop client.

use super::{open_identity, IdentityState};

/// Computes a comparison number from this identity and the found profile's Peer ID.
#[tauri::command]
pub fn contact_safety_number(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<String, String> {
    let (identity, _) = open_identity(&state)?;
    dyapp_identity::safety::safety_number_for_peer_ids(&identity.peer_id(), &peer_id)
        .ok_or_else(|| "invalid Peer ID".to_owned())
}
