//! Contact safety-number command for a verified profile shown by the desktop client.

use super::IdentityState;
use qrcodegen::{QrCode, QrCodeEcc};

#[cfg(unix)]
pub fn camera_permission(
    webview: tauri::Webview,
    kind: tauri::webview::PermissionKind,
) -> tauri::webview::PermissionResponse {
    match kind {
        tauri::webview::PermissionKind::Camera if webview.label() == "main" => {
            tauri::webview::PermissionResponse::Allow
        }
        _ => tauri::webview::PermissionResponse::Default,
    }
}

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
    let lease = state.lease()?;
    matrix_for_peer_ids(&lease.active().0.peer_id(), &peer_id)
}

fn scan_frame(
    width: usize,
    height: usize,
    pixels: &[u8],
    mine: &str,
    theirs: &str,
) -> Result<&'static str, String> {
    const MAX_SIDE: usize = 768;
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || pixels.len() != width * height
    {
        return Err("invalid QR image dimensions".to_owned());
    }
    let expected = dyapp_identity::safety::qr_payload_for_peer_ids(theirs, mine)
        .ok_or_else(|| "invalid Peer ID".to_owned())?;
    let mut decoder = quircs::Quirc::default();
    let mut result = None;
    for code in decoder.identify(width, height, pixels) {
        if result.is_some() {
            return Ok("multiple");
        }
        let status = match code.ok().and_then(|code| code.decode().ok()) {
            None => "unreadable",
            Some(data)
                if data.payload.len() != expected.len()
                    || data.payload.first() != expected.first() =>
            {
                "invalid"
            }
            Some(data)
                if dyapp_identity::safety::qr_matches_peer_ids(&data.payload, mine, theirs) =>
            {
                "matched"
            }
            Some(_) => "foreign",
        };
        result = Some(status);
    }
    Ok(result.unwrap_or("notFound"))
}

/// Decodes one bounded grayscale frame against the active identity and selected contact.
/// A match is only a comparison result; it never writes a verified-contact mark.
#[tauri::command]
pub fn scan_contact_qr(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
    width: usize,
    height: usize,
    pixels: Vec<u8>,
) -> Result<String, String> {
    let lease = state.lease()?;
    scan_frame(
        width,
        height,
        &pixels,
        &lease.active().0.peer_id(),
        &peer_id,
    )
    .map(str::to_owned)
}

/// Computes a comparison number from this identity and the found profile's Peer ID.
#[tauri::command]
pub fn contact_safety_number(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<String, String> {
    let lease = state.lease()?;
    dyapp_identity::safety::safety_number_for_peer_ids(&lease.active().0.peer_id(), &peer_id)
        .ok_or_else(|| "invalid Peer ID".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{matrix_for_peer_ids, scan_frame, QrMatrix};
    use qrcodegen::{QrCode, QrCodeEcc};

    fn grayscale(matrix: &QrMatrix) -> (usize, Vec<u8>) {
        let size = matrix.size as usize;
        let side = (size + 8) * 6;
        let mut pixels = vec![255; side * side];
        for (index, black) in matrix.modules.iter().enumerate() {
            if *black {
                let x = (index % size + 4) * 6;
                let y = (index / size + 4) * 6;
                for row in y..y + 6 {
                    pixels[row * side + x..row * side + x + 6].fill(0);
                }
            }
        }
        (side, pixels)
    }

    #[test]
    fn qr_matrix_binds_both_ordered_peer_ids() {
        let alice = "aa".repeat(32);
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

    #[test]
    fn qr_scan_rejects_wrong_pair_bad_version_and_multiple_codes() {
        let alice = "11".repeat(32);
        let bob = "22".repeat(32);
        let carol = "33".repeat(32);
        let matrix = matrix_for_peer_ids(&bob, &alice).unwrap();
        let (side, pixels) = grayscale(&matrix);
        assert_eq!(
            scan_frame(side, side, &pixels, &alice, &bob).unwrap(),
            "matched"
        );
        assert_eq!(
            scan_frame(side, side, &pixels, &bob, &alice).unwrap(),
            "foreign"
        );
        assert_eq!(
            scan_frame(side, side, &pixels, &alice, &carol).unwrap(),
            "foreign"
        );
        assert_eq!(
            scan_frame(side, side, &vec![255; side * side], &alice, &bob).unwrap(),
            "notFound"
        );
        assert!(scan_frame(side, side, &pixels[..side], &alice, &bob).is_err());
        assert!(scan_frame(769, side, &pixels, &alice, &bob).is_err());

        let mut bad = dyapp_identity::safety::qr_payload_for_peer_ids(&bob, &alice).unwrap();
        bad[0] = 2;
        let qr = QrCode::encode_binary(&bad, QrCodeEcc::Medium).unwrap();
        let qr = &qr;
        let bad_matrix = QrMatrix {
            size: qr.size(),
            modules: (0..qr.size())
                .flat_map(|y| (0..qr.size()).map(move |x| qr.get_module(x, y)))
                .collect(),
        };
        let (bad_side, bad_pixels) = grayscale(&bad_matrix);
        assert_eq!(
            scan_frame(bad_side, bad_side, &bad_pixels, &alice, &bob).unwrap(),
            "invalid"
        );

        let wide = side * 2;
        let mut two = vec![255; wide * side];
        for y in 0..side {
            let row = &pixels[y * side..(y + 1) * side];
            two[y * wide..y * wide + side].copy_from_slice(row);
            two[y * wide + side..(y + 1) * wide].copy_from_slice(row);
        }
        assert_eq!(
            scan_frame(wide, side, &two, &alice, &bob).unwrap(),
            "multiple"
        );
    }
}
