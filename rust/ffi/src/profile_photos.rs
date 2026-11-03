//! The profile editor's own photos in the unpublished draft, the same commands as
//! clients/desktop/profile_photos.rs; see docs/design/profile-photos.md. The app re-encodes a
//! picked file, so a full image arrives as a bare JPEG of at most 2048 px with its 320 px
//! thumbnail; the frame headers are checked here, the bytes and count in the client store.

use crate::preview::{frame_size, MAX_SIDE};
use crate::profile_draft::database;
use crate::{CoreError, IdentityKey};
use dyapp_client::store::{Error, Store};
use dyapp_profile::Profile;
use std::sync::Arc;

/// Longest side of a full image, the first size the editor tries.
const MAX_FULL_SIDE: u32 = 2048;

fn store(dir: &str, key: &IdentityKey) -> Result<Store, CoreError> {
    Store::open(&database(dir)?.join("client.db"), &key.identity).map_err(code)
}

fn code(error: Error) -> CoreError {
    CoreError::Failed(match error {
        Error::PhotoRefused => "photoRefused".into(),
        error => error.to_string(),
    })
}

/// The draft's thumbnail hashes in order; the first photo is the main one.
fn thumbnails(draft: &Profile) -> Vec<Vec<u8>> {
    draft
        .photos
        .iter()
        .map(|photo| photo.thumbnail.clone())
        .collect()
}

fn checked(jpeg: &[u8], side: u32) -> Result<(), CoreError> {
    match frame_size(jpeg) {
        Ok((width, height)) if width > 0 && height > 0 && width.max(height) <= side => Ok(()),
        _ => Err(CoreError::Failed("corrupted".into())),
    }
}

fn index(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// The draft's thumbnail hashes in order, empty without a draft.
#[uniffi::export]
pub fn profile_photos(dir: String, key: Arc<IdentityKey>) -> Result<Vec<Vec<u8>>, CoreError> {
    let draft = store(&dir, &key)?.profile_draft().map_err(code)?;
    Ok(draft.as_ref().map(thumbnails).unwrap_or_default())
}

/// Appends a photo to the draft and returns the thumbnail hashes. Errors are `corrupted` for a
/// frame header that is not a JPEG within 2048 px (320 px for the thumbnail), `photoRefused` for a
/// seventh photo or an oversized blob, or a storage message.
#[uniffi::export]
pub fn add_profile_photo(
    dir: String,
    key: Arc<IdentityKey>,
    full: Vec<u8>,
    thumbnail: Vec<u8>,
) -> Result<Vec<Vec<u8>>, CoreError> {
    checked(&full, MAX_FULL_SIDE)?;
    checked(&thumbnail, MAX_SIDE)?;
    let store = store(&dir, &key)?;
    Ok(thumbnails(
        &store.add_profile_photo(&full, &thumbnail).map_err(code)?,
    ))
}

/// Drops the photo at `index` from the draft; the next publish drops it from the network.
/// Errors are `photoRefused` for an index out of range or a storage message.
#[uniffi::export]
pub fn remove_profile_photo(
    dir: String,
    key: Arc<IdentityKey>,
    index: u32,
) -> Result<Vec<Vec<u8>>, CoreError> {
    let draft = store(&dir, &key)?.remove_profile_photo(self::index(index));
    Ok(thumbnails(&draft.map_err(code)?))
}

/// Moves the photo at `from` to `to`. Errors as in [`remove_profile_photo`].
#[uniffi::export]
pub fn move_profile_photo(
    dir: String,
    key: Arc<IdentityKey>,
    from: u32,
    to: u32,
) -> Result<Vec<Vec<u8>>, CoreError> {
    let draft = store(&dir, &key)?.move_profile_photo(index(from), index(to));
    Ok(thumbnails(&draft.map_err(code)?))
}

/// One stored blob of the owner's photos by hash. Errors are `notFound` or a storage message.
#[uniffi::export]
pub fn profile_photo(
    dir: String,
    key: Arc<IdentityKey>,
    hash: Vec<u8>,
) -> Result<Vec<u8>, CoreError> {
    store(&dir, &key)?
        .profile_photo(&hash)
        .map_err(code)?
        .ok_or_else(|| CoreError::Failed("notFound".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;
    use image::RgbImage;

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        JpegEncoder::new(&mut bytes)
            .encode_image(&RgbImage::new(width, height))
            .unwrap();
        bytes
    }

    fn err<T: std::fmt::Debug>(result: Result<T, CoreError>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn the_editor_adds_moves_and_removes_jpegs_within_the_bounds() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("dyapp-ffi-photos-{nanos}"));
        std::fs::create_dir_all(&path).unwrap();
        let dir = path.to_str().unwrap().to_owned();
        let key = IdentityKey::generate();
        let add = |full: Vec<u8>, thumbnail: Vec<u8>| {
            add_profile_photo(dir.clone(), key.clone(), full, thumbnail)
        };
        assert!(profile_photos(dir.clone(), key.clone()).unwrap().is_empty());
        let (small, large) = (jpeg(4, 2), jpeg(MAX_FULL_SIDE, 2));
        assert_eq!(err(add(b"GIF89a".to_vec(), small.clone())), "corrupted");
        assert_eq!(
            err(add(jpeg(MAX_FULL_SIDE + 1, 2), small.clone())),
            "corrupted"
        );
        assert_eq!(err(add(small.clone(), jpeg(2, MAX_SIDE + 1))), "corrupted");
        let first = add(large.clone(), small.clone()).unwrap();
        let second = add(large.clone(), jpeg(2, 4)).unwrap();
        assert_eq!(second[0], first[0]);
        assert_eq!(
            profile_photo(dir.clone(), key.clone(), second[1].clone()).unwrap(),
            jpeg(2, 4)
        );
        let moved = move_profile_photo(dir.clone(), key.clone(), 1, 0).unwrap();
        assert_eq!(moved, [second[1].clone(), second[0].clone()]);
        assert_eq!(
            err(move_profile_photo(dir.clone(), key.clone(), 2, 0)),
            "photoRefused"
        );
        for _ in 2..6 {
            add(large.clone(), small.clone()).unwrap();
        }
        assert_eq!(err(add(large.clone(), small.clone())), "photoRefused");
        let left = remove_profile_photo(dir.clone(), key.clone(), 0).unwrap();
        assert_eq!(left.len(), 5);
        assert_eq!(left[0], second[0]);
        assert_eq!(
            err(remove_profile_photo(dir.clone(), key.clone(), 9)),
            "photoRefused"
        );
        // A removed blob stays until a publish, and an unknown hash is not found.
        assert!(profile_photo(dir.clone(), key.clone(), moved[0].clone()).is_ok());
        assert_eq!(
            err(profile_photo(dir.clone(), key.clone(), vec![0; 32])),
            "notFound"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}
