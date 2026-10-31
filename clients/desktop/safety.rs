//! Contact safety-number command for a verified profile shown by the desktop client.

use super::{open_identity, IdentityState};
use qrcodegen::{QrCode, QrCodeEcc};

#[derive(serde::Serialize)]
pub struct QrMatrix {
    size: i32,
    modules: Vec<bool>,
}

fn matrix_for_peer_ids(mine: &str, theirs: &str) -> Result<QrMatrix, String> {
    let payload = dyapp_identity::safety::qr_payload_for_peer_ids(mine, theirs)
        .ok_or_else(|| "invalid Peer ID".to_owned())?;
    let qr = QrCode::encode_binary(&payload, QrCodeEcc::Medium)
        .map_err(|_| "could not encode contact QR".to_owned())?;
    let size = qr.size();
    let qr = &qr;
    let modules = (0..size)
        .flat_map(|y| (0..size).map(move |x| qr.get_module(x, y)))
        .collect();
    Ok(QrMatrix { size, modules })
}

/// Returns a QR matrix for the active identity and selected contact.
#[tauri::command]
pub fn contact_safety_qr(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<QrMatrix, String> {
    let (identity, _) = open_identity(&state)?;
    matrix_for_peer_ids(&identity.peer_id(), &peer_id)
}

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

#[cfg(test)]
mod tests {
    use super::matrix_for_peer_ids;

    #[test]
    fn qr_matrix_binds_both_ordered_peer_ids() {
        let alice = "11".repeat(32);
        let bob = "22".repeat(32);
        let carol = "33".repeat(32);
        let shown = matrix_for_peer_ids(&alice, &bob).unwrap();
        assert_eq!(shown.modules.len(), (shown.size * shown.size) as usize);
        assert!(shown.modules[0]); // top-left finder pattern
        assert_ne!(
            shown.modules,
            matrix_for_peer_ids(&bob, &alice).unwrap().modules
        );
        assert_ne!(
            shown.modules,
            matrix_for_peer_ids(&alice, &carol).unwrap().modules
        );
        assert!(matrix_for_peer_ids(&alice.to_uppercase(), &bob).is_err());
    }
}
