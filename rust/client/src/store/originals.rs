//! Originals of chat media this device opened, in `originals/` of the identity's folder, so they
//! outlive a restart and the nodes' retention. A file is sealed under a key derived from the
//! identity's sealing secret, which lies outside the Store: the Store holds each attachment's key
//! in the clear, so the node's blob would not protect it. Files follow the attachment rows, so
//! clearing a chat erases them, and removing an identity removes the folder.

use super::{Error, Store};
use crate::attachment::{self, Attachment};
use crate::chat::Body;
use dyapp_identity::sha256;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const FOLDER: &str = "originals";
const SETTING: &str = "kept_originals";
const MIB: u64 = 1024 * 1024;
const KEY_LABEL: &[u8] = b"dyapp-kept-original-v1";

mod settings;
#[cfg(test)]
mod stickers;
pub use settings::MediaCacheConfig;

fn file_name(blob_sha256: &[u8; 32]) -> String {
    dyapp_identity::hex(blob_sha256)
}

fn path(dir: &Path, blob_sha256: &[u8; 32]) -> PathBuf {
    dir.join(FOLDER).join(file_name(blob_sha256))
}

// One key per file, so the chunk nonces of `attachment::encrypt` never repeat under a key: a blob
// hash fixes its plaintext, and sealing the same plaintext again gives the same file.
fn file_key(seal_key: &[u8; 32], blob_sha256: &[u8; 32]) -> [u8; 32] {
    sha256(&[KEY_LABEL, seal_key, blob_sha256].concat())
}

/// Whether this device keeps the original under this blob hash.
#[must_use]
pub fn is_kept(dir: &Path, blob_sha256: &[u8; 32]) -> bool {
    path(dir, blob_sha256).is_file()
}

/// Writes an opened original. Sweep afterwards: the limit applies then.
///
/// # Errors
///
/// Returns an error if encrypting the original, creating its directory, or
/// writing the file fails. A failed write may leave a partial file.
pub fn keep(
    dir: &Path,
    seal_key: &[u8; 32],
    photo: &Attachment,
    plaintext: &[u8],
) -> io::Result<()> {
    let sealed = attachment::encrypt(&file_key(seal_key, &photo.blob_sha256), plaintext)
        .map_err(io::Error::other)?;
    fs::create_dir_all(dir.join(FOLDER))?;
    fs::write(path(dir, &photo.blob_sha256), sealed)
}

/// The kept original, marked as just used for eviction. A missing file is `None`, and so is a torn
/// or foreign one, which goes, so that the photo stops counting as kept: the caller falls back to
/// the nodes, which the automatic path may only do under the download policy.
#[must_use]
pub fn read(dir: &Path, seal_key: &[u8; 32], photo: &Attachment) -> Option<Vec<u8>> {
    let path = path(dir, &photo.blob_sha256);
    let sealed = fs::read(&path).ok()?;
    let opened = attachment::decrypt(&file_key(seal_key, &photo.blob_sha256), &sealed);
    let Some(plaintext) = opened
        .ok()
        .filter(|plaintext| sha256(plaintext) == photo.plaintext_sha256)
    else {
        let _ = fs::remove_file(path);
        return None;
    };
    // A failure here only makes this file an earlier candidate for eviction.
    let _ = File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
    Some(plaintext)
}

// Removes files no attachment names, then the least recently used past `limit` bytes.
fn prune(dir: &Path, known: &HashSet<String>, limit: u64) -> io::Result<()> {
    let entries = match fs::read_dir(dir.join(FOLDER)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut held = Vec::new();
    for entry in entries {
        let entry = entry?;
        if known.contains(entry.file_name().to_string_lossy().as_ref()) {
            let meta = entry.metadata()?;
            held.push((meta.modified()?, meta.len(), entry.path()));
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    held.sort_by_key(|file| Reverse(file.0));
    let mut total = 0;
    for (_, size, path) in held {
        total += size;
        if total > limit {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

impl Store {
    pub(crate) fn attachment_blobs(&self) -> Result<HashSet<String>, Error> {
        let mut query = self
            .db
            .prepare("SELECT `media_kind`, `descriptor` FROM `chat_attachments`")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut blobs = HashSet::new();
        for row in rows {
            let (kind, bytes) = row?;
            let hash = match kind {
                1 => {
                    let body = Body::decode(&bytes).map_err(|_| Error::InboxConflict)?;
                    let message = body.attachment_message().ok_or(Error::InboxConflict)?;
                    Some(message.attachment.blob_sha256)
                }
                2 => crate::chat::sticker::Sticker::decode(&bytes)
                    .map_err(|_| Error::InboxConflict)?
                    .download()
                    .map(|attachment| attachment.blob_sha256),
                _ => return Err(Error::InboxConflict),
            };
            if let Some(hash) = hash {
                blobs.insert(file_name(&hash));
            }
        }
        Ok(blobs)
    }

    /// Erases kept originals of attachments gone from every chat and what the limit no longer
    /// admits. Call after keeping an original, clearing a chat or changing the limit.
    // ponytail: reads every descriptor, an index of blob hashes if histories outgrow this.
    ///
    /// # Errors
    ///
    /// Returns an error if cache settings or stored attachment descriptors cannot
    /// be read or validated, or filesystem enumeration, metadata, or removal fails.
    /// Earlier file removals remain applied when a later operation fails.
    pub fn sweep_originals(&self, dir: &Path) -> io::Result<()> {
        let config = self.media_cache_config().map_err(io::Error::other)?;
        let known = self.attachment_blobs().map_err(io::Error::other)?;
        prune(
            dir,
            &known,
            if config.enabled {
                config.limit_bytes
            } else {
                0
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{file_name, is_kept, keep, path, prune, read};
    use crate::attachment::{seal, Attachment};
    use dyapp_identity::Identity;
    use std::collections::HashSet;
    use std::fs::{self, File};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn folder() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("originals-{}", Identity::generate().peer_id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn photo(bytes: &[u8]) -> Attachment {
        seal(bytes).unwrap().0
    }

    #[test]
    fn a_kept_original_opens_only_under_its_key_and_hash() {
        let dir = folder();
        let bytes = b"the plaintext original".repeat(10_000);
        let photo = photo(&bytes);
        assert_eq!(read(&dir, &[1; 32], &photo), None);
        keep(&dir, &[1; 32], &photo, &bytes).unwrap();
        assert!(is_kept(&dir, &photo.blob_sha256));
        assert_eq!(read(&dir, &[1; 32], &photo), Some(bytes.clone()));
        assert_eq!(read(&dir, &[2; 32], &photo), None);
        assert!(!is_kept(&dir, &photo.blob_sha256));

        keep(&dir, &[1; 32], &photo, &bytes).unwrap();
        let file = fs::read(path(&dir, &photo.blob_sha256)).unwrap();
        assert!(!file.windows(13).any(|part| part == b"the plaintext"));
        fs::write(path(&dir, &photo.blob_sha256), &file[..file.len() / 2]).unwrap();
        assert_eq!(read(&dir, &[1; 32], &photo), None);

        let mut other = photo.clone();
        other.plaintext_sha256 = [0; 32];
        keep(&dir, &[1; 32], &photo, &bytes).unwrap();
        assert_eq!(read(&dir, &[1; 32], &other), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_sweep_drops_unknown_files_then_the_least_recently_used() {
        let dir = folder();
        let bytes = [7; 1024];
        let photos = [photo(&bytes), photo(&bytes), photo(&bytes), photo(&bytes)];
        let now = SystemTime::now();
        for (age, photo) in (1..).zip(&photos) {
            keep(&dir, &[1; 32], photo, &bytes).unwrap();
            File::options()
                .write(true)
                .open(path(&dir, &photo.blob_sha256))
                .unwrap()
                .set_modified(now - Duration::from_secs(age * 60))
                .unwrap();
        }
        let size = fs::metadata(path(&dir, &photos[0].blob_sha256))
            .unwrap()
            .len();
        // The oldest was read last, so the next to oldest is the one past the limit.
        assert!(read(&dir, &[1; 32], &photos[3]).is_some());
        let known: HashSet<_> = photos[1..]
            .iter()
            .map(|photo| file_name(&photo.blob_sha256))
            .collect();
        prune(&dir, &known, size * 2).unwrap();
        let kept = photos.iter().map(|p| is_kept(&dir, &p.blob_sha256));
        assert_eq!(kept.collect::<Vec<_>>(), [false, true, false, true]);

        prune(&dir, &known, 0).unwrap();
        assert!(!photos.iter().any(|p| is_kept(&dir, &p.blob_sha256)));
        fs::remove_dir_all(&dir).unwrap();
        prune(&dir, &known, 0).unwrap();
    }
}
