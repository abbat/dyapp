//! Local sticker commands keep the identity lease through blocking validation and storage.
use crate::{chats, IdentityState};
use dyapp_client::{
    sticker_pack,
    store::{sticker_packs, Store},
};
use dyapp_identity::Identity;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackView {
    id: [u8; 32],
    bytes: u64,
    name: Option<String>,
    stickers: Option<usize>,
    error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageView {
    entries: Vec<PackView>,
    next: Option<[u8; 32]>,
    used_bytes: u64,
    unrecognized_files: u64,
}
impl From<sticker_packs::list::Page> for PageView {
    fn from(page: sticker_packs::list::Page) -> Self {
        Self {
            entries: page
                .entries
                .into_iter()
                .map(|entry| {
                    let (name, stickers, error) = match entry.details {
                        Ok(details) => (Some(details.name), Some(details.stickers), None),
                        Err(error) => (None, None, Some(error.to_string())),
                    };
                    PackView {
                        id: entry.id,
                        bytes: entry.bytes,
                        name,
                        stickers,
                        error,
                    }
                })
                .collect(),
            next: page.next,
            used_bytes: page.used_bytes,
            unrecognized_files: page.unrecognized_files,
        }
    }
}
fn owner_matches(identity: &Identity, expected: &str) -> Result<(), String> {
    if identity.peer_id() != expected {
        return Err("Identity changed".into());
    }
    Ok(())
}
fn local_store(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    sessions: &chats::Sessions,
) -> Result<(Store, zeroize::Zeroizing<[u8; 32]>), String> {
    let store = chats::open(dir, identity)?;
    // First imports/list calls must not generate competing sealing secrets.
    let _guard = sessions.0.lock().map_err(|_| "Session state unavailable")?;
    let key = crate::sealing_key(root, dir, &store)?;
    Ok((store, key))
}
fn import(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    sessions: &chats::Sessions,
    bytes: &[u8],
) -> Result<[u8; 32], String> {
    // The shared TAR reader applies its 8 MiB bound before parsing or allocating files.
    let pack = sticker_pack::import_tar(bytes)?;
    let (mut store, key) = local_store(root, dir, identity, sessions)?;
    store
        .save_sticker_pack(&key, &pack)
        .map_err(|error| error.to_string())?;
    Ok(*pack.id())
}

/// The caller names its viewed identity so a delayed invocation cannot target another slot.
#[tauri::command]
pub(crate) async fn import_sticker_pack(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    bytes: Vec<u8>,
) -> Result<[u8; 32], String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let root = crate::config_dir(&app)?;
    let sessions = chats::Sessions(sessions.inner().0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        import(&root, dir, identity, &sessions, &bytes)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Authentication and WebP validation run off the UI thread, one pack at a time.
#[tauri::command]
pub(crate) async fn saved_sticker_packs(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    after: Option<[u8; 32]>,
) -> Result<PageView, String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let root = crate::config_dir(&app)?;
    let sessions = chats::Sessions(sessions.inner().0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        let (store, key) = local_store(&root, dir, identity, &sessions)?;
        store
            .saved_sticker_packs(&key, after.as_ref())
            .map(PageView::from)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
#[path = "stickers/tests.rs"]
mod native_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collection_errors_remain_explicit_in_page_json() {
        let page = PageView::from(sticker_packs::list::Page {
            entries: vec![sticker_packs::list::Entry {
                id: [3; 32],
                bytes: 12,
                details: Err(sticker_packs::Error::Invalid),
            }],
            next: Some([4; 32]),
            used_bytes: 19,
            unrecognized_files: 1,
        });
        let json = serde_json::to_value(page).unwrap();
        assert_eq!(json["usedBytes"], 19);
        assert_eq!(json["unrecognizedFiles"], 1);
        assert_eq!(json["entries"][0]["bytes"], 12);
        assert!(json["entries"][0]["name"].is_null());
        assert!(json["entries"][0]["error"]
            .as_str()
            .unwrap()
            .contains("invalid"));
        assert_eq!(json["next"][0], 4);
    }

    #[test]
    fn invalid_import_does_not_create_local_state_and_owner_is_checked() {
        let identity = Identity::generate();
        let dir = std::env::current_dir()
            .unwrap()
            .join("build")
            .join(format!("sticker-command-{}", identity.peer_id()));
        let sessions = chats::Sessions::default();
        for (bytes, reason) in [
            (Vec::new(), "missing TAR end blocks"),
            (vec![0; 8 * 1024 * 1024 + 1], "too large"),
        ] {
            assert!(import(&dir, &dir, &identity, &sessions, &bytes)
                .unwrap_err()
                .contains(reason));
            assert!(!dir.exists());
        }
        assert!(owner_matches(&identity, &identity.peer_id()).is_ok());
        assert!(owner_matches(&identity, &Identity::generate().peer_id()).is_err());
    }
}
