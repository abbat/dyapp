//! The owner's photo blobs, one `settings` row per hash, so a publish or a repair needs no original
//! file. A removed photo stays here until the next publish: the live profile still lists it.

use super::{Error, Store};
use dyapp_identity::sha256;
use dyapp_profile::{Photo, Profile};
use rusqlite::{params, OptionalExtension};

/// Most photos this client publishes; the schema allows more, the node quota does not.
pub const MAX_PROFILE_PHOTOS: usize = 6;
/// Largest full image, small enough to stay a whole-copy blob.
pub const MAX_PHOTO_BYTES: usize = 512 * 1024;
/// Largest thumbnail.
pub const MAX_THUMBNAIL_BYTES: usize = 24 * 1024;

const PREFIX: &str = "profile_photo:";

fn key(hash: &[u8]) -> String {
    let hex: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{PREFIX}{hex}")
}

/// Whether `jpeg` is within `limit` and starts as a JPEG; a blob from a node passes this too.
pub fn fits(jpeg: &[u8], limit: usize) -> bool {
    jpeg.len() <= limit && jpeg.starts_with(&[0xFF, 0xD8, 0xFF])
}

impl Store {
    fn save_profile_blob(&self, data: &[u8]) -> Result<Vec<u8>, Error> {
        let hash = sha256(data).to_vec();
        self.db.execute(
            "INSERT OR IGNORE INTO `settings` (`key`, `format_version`, `value`)
             VALUES (?1, 1, ?2)",
            params![key(&hash), data],
        )?;
        Ok(hash)
    }

    /// Stores a photo and appends it to the draft; the caller has checked its pixels. Refuses a
    /// non-JPEG, an oversized blob or a seventh photo.
    pub fn add_profile_photo(&self, full: &[u8], thumbnail: &[u8]) -> Result<Profile, Error> {
        let mut draft = self.profile_draft()?.unwrap_or_default();
        if draft.photos.len() >= MAX_PROFILE_PHOTOS
            || !fits(full, MAX_PHOTO_BYTES)
            || !fits(thumbnail, MAX_THUMBNAIL_BYTES)
        {
            return Err(Error::PhotoRefused);
        }
        draft.photos.push(Photo {
            full: vec![self.save_profile_blob(full)?],
            thumbnail: self.save_profile_blob(thumbnail)?,
        });
        self.save_profile_draft(&draft)?;
        Ok(draft)
    }

    /// Drops the photo at `index` from the draft. Its blobs stay until a publish succeeds.
    pub fn remove_profile_photo(&self, index: usize) -> Result<Profile, Error> {
        let mut draft = self.profile_draft()?.unwrap_or_default();
        if index >= draft.photos.len() {
            return Err(Error::PhotoRefused);
        }
        draft.photos.remove(index);
        self.save_profile_draft(&draft)?;
        Ok(draft)
    }

    /// Moves the photo at `from` to `to`; the first photo is the main one.
    pub fn move_profile_photo(&self, from: usize, to: usize) -> Result<Profile, Error> {
        let mut draft = self.profile_draft()?.unwrap_or_default();
        if from >= draft.photos.len() || to >= draft.photos.len() {
            return Err(Error::PhotoRefused);
        }
        let photo = draft.photos.remove(from);
        draft.photos.insert(to, photo);
        self.save_profile_draft(&draft)?;
        Ok(draft)
    }

    /// The stored blob of `hash`.
    pub fn profile_photo(&self, hash: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT `value` FROM `settings` WHERE `key` = ?1",
                [key(hash)],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn profile_photo_keys(&self) -> Result<Vec<String>, Error> {
        let mut query = self
            .db
            .prepare("SELECT `key` FROM `settings` WHERE `key` LIKE ?1")?;
        let rows = query.query_map([format!("{PREFIX}%")], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Whether any blob is stored, so nodes may hold a list that needs replacing.
    pub fn has_profile_photos(&self) -> Result<bool, Error> {
        Ok(!self.profile_photo_keys()?.is_empty())
    }

    /// Deletes every blob but `hashes`, after the profile that lists them is published.
    pub fn retain_profile_photos(&self, hashes: &[Vec<u8>]) -> Result<(), Error> {
        let wanted: Vec<String> = hashes.iter().map(|hash| key(hash)).collect();
        for stored in self.profile_photo_keys()? {
            if !wanted.contains(&stored) {
                self.db
                    .execute("DELETE FROM `settings` WHERE `key` = ?1", [stored])?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_db_path;
    use super::*;
    use dyapp_identity::Identity;

    fn jpeg(seed: u8, bytes: usize) -> Vec<u8> {
        let mut data = vec![0xFF, 0xD8, 0xFF, seed];
        data.resize(bytes, seed);
        data
    }

    #[test]
    fn photos_enter_the_draft_within_limits_and_leave_after_a_publish() {
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        assert!(!store.has_profile_photos().unwrap());
        for (full, thumbnail) in [
            (b"not a jpeg".to_vec(), jpeg(1, 8)),
            (jpeg(1, 8), b"not a jpeg".to_vec()),
            (jpeg(1, MAX_PHOTO_BYTES + 1), jpeg(2, 8)),
            (jpeg(1, 8), jpeg(2, MAX_THUMBNAIL_BYTES + 1)),
        ] {
            assert!(matches!(
                store.add_profile_photo(&full, &thumbnail),
                Err(Error::PhotoRefused)
            ));
        }
        assert!(!store.has_profile_photos().unwrap());

        let full = jpeg(1, MAX_PHOTO_BYTES);
        let thumbnail = jpeg(2, MAX_THUMBNAIL_BYTES);
        let draft = store.add_profile_photo(&full, &thumbnail).unwrap();
        let photo = draft.photos[0].clone();
        assert_eq!(store.profile_photo(&photo.full[0]).unwrap(), Some(full));
        assert_eq!(
            store.profile_photo(&photo.thumbnail).unwrap(),
            Some(thumbnail)
        );

        // Refusal keeps the first photo; the sixth fits and the seventh does not.
        for seed in 3..8 {
            store
                .add_profile_photo(&jpeg(seed, 8), &jpeg(2, 8))
                .unwrap();
        }
        assert!(matches!(
            store.add_profile_photo(&jpeg(9, 8), &jpeg(2, 8)),
            Err(Error::PhotoRefused)
        ));

        // A removed photo stays stored until a publish retains the rest.
        assert!(matches!(
            store.remove_profile_photo(6),
            Err(Error::PhotoRefused)
        ));
        let draft = store.remove_profile_photo(0).unwrap();
        assert_eq!(draft.photos.len(), 5);
        assert!(matches!(
            store.move_profile_photo(0, 5),
            Err(Error::PhotoRefused)
        ));
        let moved = store.move_profile_photo(4, 0).unwrap();
        assert_eq!(moved.photos[0], draft.photos[4]);
        assert_eq!(moved.photos[1], draft.photos[0]);
        assert!(store.profile_photo(&photo.full[0]).unwrap().is_some());
        store
            .retain_profile_photos(std::slice::from_ref(&photo.thumbnail))
            .unwrap();
        assert!(store.profile_photo(&photo.full[0]).unwrap().is_none());
        assert!(store.profile_photo(&photo.thumbnail).unwrap().is_some());
        store.retain_profile_photos(&[]).unwrap();
        assert!(!store.has_profile_photos().unwrap());
        // Other settings survive: the draft is one of them.
        assert_eq!(store.profile_draft().unwrap().unwrap().photos.len(), 5);
    }
}
