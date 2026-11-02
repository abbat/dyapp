//! The profile editor's photo commands over the client store. The page re-encodes a picked file,
//! so what arrives here is a bare JPEG with its thumbnail; the pixel bound is checked here because
//! the client core has no image decoder.

use std::path::Path;

use dyapp_client::store::{Error, Store};
use dyapp_identity::Identity;
use dyapp_profile::Profile;

use crate::chat_photos::check_image;
use crate::profile::{self, View};

fn change(
    dir: &Path,
    identity: &Identity,
    edit: impl FnOnce(&Store) -> Result<Profile, Error>,
) -> Result<View, String> {
    let store = profile::open(dir, identity)?;
    let draft = edit(&store).map_err(|error| match error {
        Error::PhotoRefused => "photoRefused".to_string(),
        error => error.to_string(),
    })?;
    profile::view(&draft, profile::today()?, Some(&store))
}

fn add(dir: &Path, identity: &Identity, full: &[u8], thumbnail: &[u8]) -> Result<View, String> {
    check_image(full)?;
    check_image(thumbnail)?;
    change(dir, identity, |store| {
        store.add_profile_photo(full, thumbnail)
    })
}

async fn on_identity<T: Send + 'static>(
    state: &crate::IdentityState,
    work: impl FnOnce(&Path, &Identity) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        work(&dir, &identity)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Adds a photo to the draft. Errors are `corrupted`, `photoRefused` or a storage message.
#[tauri::command]
pub(crate) async fn add_profile_photo(
    state: tauri::State<'_, crate::IdentityState>,
    full: Vec<u8>,
    thumbnail: Vec<u8>,
) -> Result<View, String> {
    on_identity(&state, move |dir, identity| {
        add(dir, identity, &full, &thumbnail)
    })
    .await
}

/// Drops a photo from the draft; the next publish drops it from the network.
#[tauri::command]
pub(crate) async fn remove_profile_photo(
    state: tauri::State<'_, crate::IdentityState>,
    index: usize,
) -> Result<View, String> {
    on_identity(&state, move |dir, identity| {
        change(dir, identity, |store| store.remove_profile_photo(index))
    })
    .await
}

/// Moves a photo within the draft; the first one is the main photo.
#[tauri::command]
pub(crate) async fn move_profile_photo(
    state: tauri::State<'_, crate::IdentityState>,
    from: usize,
    to: usize,
) -> Result<View, String> {
    on_identity(&state, move |dir, identity| {
        change(dir, identity, |store| store.move_profile_photo(from, to))
    })
    .await
}

/// One stored blob of the owner's photos, for the editor.
#[tauri::command]
pub(crate) async fn profile_photo(
    state: tauri::State<'_, crate::IdentityState>,
    hash: Vec<u8>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = on_identity(&state, move |dir, identity| {
        profile::open(dir, identity)?
            .profile_photo(&hash)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "notFound".to_string())
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::{add, change, Identity};
    use image::{ImageFormat, RgbImage};
    use std::io::Cursor;

    fn encoded(format: ImageFormat, width: u32) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        RgbImage::new(width, 2)
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn the_editor_lists_thumbnails_in_order_and_refuses_what_is_not_a_jpeg() {
        let dir = std::env::temp_dir().join(format!("dyapp-photos-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let identity = Identity::generate();
        let (first, second) = (
            encoded(ImageFormat::Jpeg, 3),
            encoded(ImageFormat::Jpeg, 300),
        );
        let refused = add(&dir, &identity, &first, &[1, 2, 3]).unwrap_err();
        assert_eq!(refused, "corrupted");
        let png = encoded(ImageFormat::Png, 3);
        assert_eq!(
            add(&dir, &identity, &png, &first).unwrap_err(),
            "photoRefused"
        );

        add(&dir, &identity, &first, &first).unwrap();
        let added = add(&dir, &identity, &second, &second).unwrap();
        let moved = change(&dir, &identity, |store| store.move_profile_photo(1, 0)).unwrap();
        assert_eq!(
            moved.draft.photos,
            [added.draft.photos[1].clone(), added.draft.photos[0].clone()]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
