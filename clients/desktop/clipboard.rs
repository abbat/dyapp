//! Sensitive safety-number clipboard operations shared by Linux and Windows.

use super::IdentityState;
use tauri_plugin_clipboard_manager::ClipboardExt;

fn grouped_number(mine: &str, theirs: &str) -> Result<String, String> {
    let digits = dyapp_identity::safety::safety_number_for_peer_ids(mine, theirs)
        .ok_or_else(|| "invalid Peer ID".to_owned())?;
    Ok(digits
        .as_bytes()
        .chunks(5)
        .map(|group| std::str::from_utf8(group).expect("ASCII safety digits"))
        .collect::<Vec<_>>()
        .join(" "))
}

/// Copies the active identity's contact number with a clipboard-history exclusion hint.
/// The returned value is the exact text to compare when the 60-second timer fires.
#[tauri::command]
pub async fn copy_safety_number(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<String, String> {
    let lease = state.lease()?;
    let number = grouped_number(&lease.active().0.peer_id(), &peer_id)?;
    tauri::async_runtime::spawn_blocking({
        let number = number.clone();
        move || {
            let _lease = lease;
            app.clipboard()
                .write_secret(number)
                .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(number)
}

/// Clears the clipboard only if its text still equals the copied safety number.
/// Reading happens off the GUI thread because the Linux clipboard may wait for its owner.
#[tauri::command]
pub async fn clear_safety_clipboard(
    app: tauri::AppHandle,
    expected: String,
) -> Result<bool, String> {
    if expected.len() != 71
        || expected.bytes().enumerate().any(|(i, byte)| {
            if (i + 1) % 6 == 0 {
                byte != b' '
            } else {
                !byte.is_ascii_digit()
            }
        })
    {
        return Err("invalid safety number".to_owned());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let clipboard = app.clipboard();
        if clipboard.read_text().ok().as_deref() != Some(expected.as_str()) {
            return Ok(false);
        }
        clipboard.clear().map_err(|e| e.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::grouped_number;

    #[test]
    fn group_copy_value_matches_displayed_number() {
        let a = "00".repeat(32);
        let b = "11".repeat(32);
        let grouped = grouped_number(&a, &b).unwrap();
        assert_eq!(grouped.len(), 71);
        assert_eq!(
            grouped.replace(' ', ""),
            dyapp_identity::safety::safety_number_for_peer_ids(&a, &b).unwrap()
        );
    }
}
