//! The owner's photo blobs, one `settings` row per hash, so a publish or a repair needs no original
//! file. A removed photo stays here until the next publish: the live profile still lists it.

use super::{Error, Store};
use dyapp_identity::sha256;
use dyapp_profile::{Photo, Profile};
use rusqlite::{params, Connection, OptionalExtension};

/// Most photos this client publishes; the schema allows more, the node quota does not.
pub const MAX_PROFILE_PHOTOS: usize = 6;
/// Largest full image: `media.shard_threshold`, the largest whole-copy blob.
pub const MAX_PHOTO_BYTES: usize = 1024 * 1024;
/// Largest thumbnail.
pub const MAX_THUMBNAIL_BYTES: usize = 24 * 1024;

const PREFIX: &str = "profile_photo:";
/// Unix second of the last keep refresh, as 8 big-endian bytes.
const KEPT: &str = "profile_photos_kept";
/// Seconds between keep refreshes.
const KEEP_INTERVAL: u64 = 86_400;

fn key(hash: &[u8]) -> String {
    let hex: String = dyapp_identity::hex(hash);
    format!("{PREFIX}{hex}")
}

/// Whether `jpeg` is within `limit` and starts as a JPEG; a blob from a node passes this too.
#[must_use]
pub fn fits(jpeg: &[u8], limit: usize) -> bool {
    jpeg.len() <= limit && jpeg.starts_with(&[0xFF, 0xD8, 0xFF])
}

impl Store {
    /// Stores a blob of the published profile under its hash, as a linked device takes it.
    ///
    /// # Errors
    ///
    /// Returns an error if storing the blob in the database fails.
    pub fn save_profile_blob(&self, data: &[u8]) -> Result<Vec<u8>, Error> {
        save_blob(&self.db, data)
    }

    /// Stores a photo and appends it to the draft; the caller has checked its pixels. Refuses a
    /// non-JPEG, an oversized blob or a seventh photo.
    ///
    /// # Errors
    ///
    /// Returns an error if the draft is corrupt or unsupported, a photo exceeds the
    /// count, size, or JPEG limits, or database access fails. Both blobs and the
    /// draft commit together; a failure leaves all three unchanged.
    pub fn add_profile_photo(&mut self, full: &[u8], thumbnail: &[u8]) -> Result<Profile, Error> {
        if !fits(full, MAX_PHOTO_BYTES) || !fits(thumbnail, MAX_THUMBNAIL_BYTES) {
            return Err(Error::PhotoRefused);
        }
        super::profile_draft::transform(&mut self.db, |db, draft| {
            if draft.photos.len() >= MAX_PROFILE_PHOTOS {
                return Err(Error::PhotoRefused);
            }
            draft.photos.push(Photo {
                full: vec![save_blob(db, full)?],
                thumbnail: save_blob(db, thumbnail)?,
            });
            Ok(())
        })
    }

    /// Drops the photo at `index` from the draft. Its blobs stay until a publish succeeds.
    ///
    /// # Errors
    ///
    /// Returns an error if the draft is corrupt or unsupported, `index` is out of
    /// bounds, or reading or saving the draft fails.
    pub fn remove_profile_photo(&mut self, index: usize) -> Result<Profile, Error> {
        self.update_profile_draft(|draft| {
            if index >= draft.photos.len() {
                return Err(Error::PhotoRefused);
            }
            draft.photos.remove(index);
            Ok(())
        })
    }

    /// Moves the photo at `from` to `to`; the first photo is the main one.
    ///
    /// # Errors
    ///
    /// Returns an error if the draft is corrupt or unsupported, either index is out
    /// of bounds, or reading or saving the draft fails.
    pub fn move_profile_photo(&mut self, from: usize, to: usize) -> Result<Profile, Error> {
        self.update_profile_draft(|draft| {
            if from >= draft.photos.len() || to >= draft.photos.len() {
                return Err(Error::PhotoRefused);
            }
            let photo = draft.photos.remove(from);
            draft.photos.insert(to, photo);
            Ok(())
        })
    }

    /// The stored blob of `hash`.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the stored blob fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the stored blob keys fails.
    pub fn has_profile_photos(&self) -> Result<bool, Error> {
        Ok(!self.profile_photo_keys()?.is_empty())
    }

    /// Whether a keep refresh is due at Unix second `now`. A clock moved back counts as elapsed, so
    /// it cannot stop the refresh for as long as it moved.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the refresh time fails or its stored value has
    /// an invalid length.
    pub fn profile_keep_due(&self, now: u64) -> Result<bool, Error> {
        let kept: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT `value` FROM `settings` WHERE `key` = ?1 AND `format_version` = 1",
                [KEPT],
                |row| row.get(0),
            )
            .optional()?;
        let Some(kept) = kept else {
            return Ok(true);
        };
        let kept: [u8; 8] = kept.try_into().map_err(|_| Error::InvalidSetting)?;
        Ok(now.abs_diff(u64::from_be_bytes(kept)) >= KEEP_INTERVAL)
    }

    /// Records a keep refresh attempted at `now`.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the refresh time to the database fails.
    pub fn profile_keep_refreshed(&self, now: u64) -> Result<(), Error> {
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![KEPT, now.to_be_bytes().to_vec()],
        )?;
        Ok(())
    }

    /// Deletes every blob but `hashes`, after the profile that lists them is published.
    ///
    /// # Errors
    ///
    /// Returns an error if listing blob keys or deleting a blob fails. Earlier
    /// deletions remain applied when a later deletion fails.
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

fn save_blob(db: &Connection, data: &[u8]) -> Result<Vec<u8>, Error> {
    let hash = sha256(data).to_vec();
    db.execute(
        "INSERT OR IGNORE INTO `settings` (`key`, `format_version`, `value`)
         VALUES (?1, 1, ?2)",
        params![key(&hash), data],
    )?;
    Ok(hash)
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
    fn photo_changes_cannot_bypass_an_independent_draft_writer() {
        let (path, owner) = (temp_db_path(), Identity::generate());
        let mut writer = Store::open(&path, &owner).unwrap();
        let mut photos = Store::open(&path, &owner).unwrap();
        let original = photos.add_profile_photo(&jpeg(1, 8), &jpeg(2, 8)).unwrap();
        photos.db.busy_timeout(std::time::Duration::ZERO).unwrap();
        writer
            .update_profile_draft(|_| {
                for result in [
                    photos.add_profile_photo(&jpeg(3, 8), &jpeg(4, 8)),
                    photos.remove_profile_photo(0),
                    photos.move_profile_photo(0, 0),
                ] {
                    assert!(matches!(
                        result,
                        Err(Error::Sql(rusqlite::Error::SqliteFailure(error, _)))
                            if error.code == rusqlite::ErrorCode::DatabaseBusy
                    ));
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(photos.profile_draft().unwrap(), Some(original));
        assert!(photos
            .profile_photo(&sha256(&jpeg(3, 8)))
            .unwrap()
            .is_none());
        assert!(photos
            .profile_photo(&sha256(&jpeg(4, 8)))
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_failed_draft_write_rolls_back_both_photo_blobs() {
        let (path, owner) = (temp_db_path(), Identity::generate());
        let mut store = Store::open(&path, &owner).unwrap();
        let mut observer = Store::open(&path, &owner).unwrap();
        let original = Profile {
            version: 7,
            ..Profile::default()
        };
        store.save_profile_draft(&original).unwrap();
        // Fail after both blob writes, when the same transaction writes the draft.
        store
            .db
            .execute_batch(
                "CREATE TEMP TRIGGER `refuse_draft` BEFORE INSERT ON `settings`
             WHEN NEW.`key` = 'profile_draft'
             BEGIN SELECT RAISE(ABORT, 'injected draft write refusal'); END;",
            )
            .unwrap();
        let (full, small) = (jpeg(1, 16), jpeg(2, 8));
        assert!(store.add_profile_photo(&full, &small).is_err());
        assert_eq!(observer.profile_draft().unwrap(), Some(original));
        for data in [&full, &small] {
            assert!(observer.profile_photo(&sha256(data)).unwrap().is_none());
        }
        // The connection-local trigger does not affect the independent writer.
        observer.db.busy_timeout(std::time::Duration::ZERO).unwrap();
        let added = observer.add_profile_photo(&full, &small).unwrap();
        assert_eq!(added.version, 7);
        assert_eq!(added.photos.len(), 1);
        assert_eq!(store.profile_draft().unwrap(), Some(added));
    }

    #[test]
    fn photo_changes_preserve_the_latest_fields_from_an_independent_editor() {
        let (path, owner) = (temp_db_path(), Identity::generate());
        let mut photos = Store::open(&path, &owner).unwrap();
        let mut editor = Store::open(&path, &owner).unwrap();
        photos.add_profile_photo(&jpeg(1, 8), &jpeg(2, 8)).unwrap();
        editor
            .update_profile_draft(|draft| {
                draft.version = 7;
                draft.pow_bits = 4;
                Ok(())
            })
            .unwrap();
        let added = photos.add_profile_photo(&jpeg(3, 8), &jpeg(4, 8)).unwrap();
        assert_eq!((added.version, added.pow_bits), (7, 4));
        editor
            .update_profile_draft(|draft| {
                draft.gender = dyapp_profile::Gender::Woman.code();
                Ok(())
            })
            .unwrap();
        let moved = photos.move_profile_photo(1, 0).unwrap();
        assert_eq!(
            moved.photos,
            [added.photos[1].clone(), added.photos[0].clone()]
        );
        assert_eq!(moved.gender, dyapp_profile::Gender::Woman.code());
        let removed = photos.remove_profile_photo(1).unwrap();
        assert_eq!(removed.photos, [added.photos[1].clone()]);
        assert_eq!((removed.version, removed.pow_bits), (7, 4));
        assert_eq!(removed.gender, dyapp_profile::Gender::Woman.code());
        assert_eq!(editor.profile_draft().unwrap(), Some(removed));
    }

    #[test]
    fn the_keep_refresh_is_due_once_a_day() {
        let owner = Identity::generate();
        let store = Store::open(&temp_db_path(), &owner).unwrap();
        assert!(store.profile_keep_due(1_000_000).unwrap());
        store.profile_keep_refreshed(1_000_000).unwrap();
        assert!(!store.profile_keep_due(1_000_000).unwrap());
        assert!(!store.profile_keep_due(1_086_399).unwrap());
        assert!(store.profile_keep_due(1_086_400).unwrap());
        assert!(store.profile_keep_due(900_000).unwrap());
        // The keep time is no photo.
        assert!(!store.has_profile_photos().unwrap());
    }

    #[test]
    fn photos_enter_the_draft_within_limits_and_leave_after_a_publish() {
        assert_eq!(MAX_PHOTO_BYTES, 1_048_576);
        let mut store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
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
