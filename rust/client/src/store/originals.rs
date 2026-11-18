//! Originals of chat media this device opened, in `originals/` of the identity's folder, so they
//! outlive a restart and the nodes' retention. A file is sealed under a key derived from the
//! identity's sealing secret, which lies outside the Store: the Store holds each attachment's key
//! in the clear, so the node's blob would not protect it. Files follow the attachment rows, so
//! clearing a chat erases them, and removing an identity removes the folder.

use super::{Error, Store};
use crate::attachment::{self, Attachment};
use crate::chat::Body;
use dyapp_identity::sha256;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const FOLDER: &str = "originals";
const SETTING: &str = "kept_originals";
const MIB: u64 = 1024 * 1024;
const CONTACT_LIMIT: u64 = 64 * MIB;
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
    held.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
    let mut total = 0u64;
    for (_, size, path) in held {
        total = total.saturating_add(size);
        if total > limit {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn attachment_blob(kind: i64, bytes: &[u8]) -> Result<Option<[u8; 32]>, Error> {
    match kind {
        1 => {
            let body = Body::decode(bytes).map_err(|_| Error::InboxConflict)?;
            let message = body.attachment_message().ok_or(Error::InboxConflict)?;
            Ok(Some(message.attachment.blob_sha256))
        }
        2 => Ok(crate::chat::sticker::Sticker::decode(bytes)
            .map_err(|_| Error::InboxConflict)?
            .download()
            .map(|attachment| attachment.blob_sha256)),
        _ => Err(Error::InboxConflict),
    }
}

fn prune_contacts(
    dir: &Path,
    known: &HashSet<String>,
    contacts: &HashMap<String, HashSet<String>>,
    limit: u64,
) -> io::Result<()> {
    let entries = match fs::read_dir(dir.join(FOLDER)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut held = Vec::new();
    let mut totals = HashMap::<String, u64>::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !known.contains(&name) {
            continue;
        }
        let meta = entry.metadata()?;
        for (peer, hashes) in contacts {
            if hashes.contains(&name) {
                let total = totals.entry(peer.clone()).or_default();
                *total = total.saturating_add(meta.len());
            }
        }
        held.push((meta.modified()?, meta.len(), entry.path(), name));
    }
    let over: HashSet<String> = totals
        .into_iter()
        .filter(|(_, total)| *total > limit)
        .map(|(peer, _)| peer)
        .collect();
    let mut ordered: Vec<_> = over.iter().cloned().collect();
    ordered.sort();
    held.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.3.cmp(&b.3)));
    let mut removed = HashSet::new();
    for peer in ordered {
        let mut total = 0u64;
        for (_, size, path, name) in &held {
            if removed.contains(name) || !contacts[&peer].contains(name) {
                continue;
            }
            total = total.saturating_add(*size);
            let shared_with_safe = contacts
                .iter()
                .any(|(other, hashes)| !over.contains(other) && hashes.contains(name));
            if total > limit && !shared_with_safe {
                fs::remove_file(path)?;
                removed.insert(name.clone());
                total = total.saturating_sub(*size);
            }
        }
    }
    Ok(())
}

impl Store {
    #[cfg(test)]
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
            if let Some(hash) = attachment_blob(kind, &bytes)? {
                blobs.insert(file_name(&hash));
            }
        }
        Ok(blobs)
    }

    fn attachment_contacts(&self) -> Result<HashMap<String, HashSet<String>>, Error> {
        let mut query = self.db.prepare(
            "SELECT `conversation_groups`.`peer_id`, `media_kind`, `descriptor`
             FROM `chat_attachments` JOIN `conversation_groups` ON
             `conversation_groups`.`group_id` = `chat_attachments`.`group_id`",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let mut contacts = HashMap::<String, HashSet<String>>::new();
        for row in rows {
            let (peer, kind, bytes) = row?;
            if let Some(hash) = attachment_blob(kind, &bytes)? {
                contacts.entry(peer).or_default().insert(file_name(&hash));
            }
        }
        Ok(contacts)
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
        let contacts = self.attachment_contacts().map_err(io::Error::other)?;
        let known: HashSet<String> = contacts.values().flatten().cloned().collect();
        let contact_limit = if config.enabled { CONTACT_LIMIT } else { 0 };
        prune_contacts(dir, &known, &contacts, contact_limit)?;
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
    use super::{file_name, is_kept, keep, path, prune, prune_contacts, read};
    use crate::attachment::{seal, Attachment};
    use dyapp_identity::Identity;
    use std::collections::{HashMap, HashSet};
    use std::fs::{self, File};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "originals-{}",
            Identity::try_generate().unwrap().peer_id()
        ));
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
        for (age, photo) in (1u64..).zip(&photos) {
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

    #[test]
    fn contact_limits_are_exact_oldest_first_and_isolated() {
        let dir = folder();
        let photos = [photo(b"first"), photo(b"second"), photo(b"third")];
        let now = SystemTime::now();
        for (age, photo) in (1u64..).zip(&photos) {
            keep(&dir, &[1; 32], photo, &[u8::try_from(age).unwrap()]).unwrap();
            File::options()
                .write(true)
                .open(path(&dir, &photo.blob_sha256))
                .unwrap()
                .set_modified(now - Duration::from_secs(age * 60))
                .unwrap();
        }
        let names: Vec<_> = photos.iter().map(|p| file_name(&p.blob_sha256)).collect();
        let known = names.iter().cloned().collect();
        let mut contacts = HashMap::new();
        contacts.insert("a".into(), names[..2].iter().cloned().collect());
        contacts.insert("b".into(), HashSet::from([names[2].clone()]));
        let size = fs::metadata(path(&dir, &photos[0].blob_sha256))
            .unwrap()
            .len();
        prune_contacts(&dir, &known, &contacts, size * 2).unwrap();
        assert!(photos.iter().all(|p| is_kept(&dir, &p.blob_sha256)));

        prune_contacts(&dir, &known, &contacts, size).unwrap();
        assert!(is_kept(&dir, &photos[0].blob_sha256));
        assert!(!is_kept(&dir, &photos[1].blob_sha256));
        assert!(is_kept(&dir, &photos[2].blob_sha256));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_shared_blob_is_pinned_for_an_under_limit_contact() {
        let dir = folder();
        let photos = [photo(b"shared"), photo(b"private")];
        let now = SystemTime::now();
        for (age, photo) in (2u64..).zip(&photos) {
            keep(&dir, &[1; 32], photo, &[u8::try_from(age).unwrap()]).unwrap();
            File::options()
                .write(true)
                .open(path(&dir, &photo.blob_sha256))
                .unwrap()
                .set_modified(now - Duration::from_secs(age * 60))
                .unwrap();
        }
        let names: Vec<_> = photos.iter().map(|p| file_name(&p.blob_sha256)).collect();
        let known = names.iter().cloned().collect();
        let mut contacts = HashMap::new();
        contacts.insert("a".into(), names.iter().cloned().collect());
        contacts.insert("b".into(), HashSet::from([names[1].clone()]));
        let size = fs::metadata(path(&dir, &photos[0].blob_sha256))
            .unwrap()
            .len();
        prune_contacts(&dir, &known, &contacts, size).unwrap();
        assert!(is_kept(&dir, &photos[0].blob_sha256));
        assert!(is_kept(&dir, &photos[1].blob_sha256));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_contact_remove_failure_is_recoverable_and_does_not_touch_another() {
        let dir = folder();
        let photos = [photo(b"first"), photo(b"second"), photo(b"other")];
        let now = SystemTime::now();
        for (age, photo) in (1u64..).zip(&photos) {
            keep(&dir, &[1; 32], photo, &[u8::try_from(age).unwrap()]).unwrap();
            File::options()
                .write(true)
                .open(path(&dir, &photo.blob_sha256))
                .unwrap()
                .set_modified(now - Duration::from_secs(age * 60))
                .unwrap();
        }
        let names: Vec<_> = photos.iter().map(|p| file_name(&p.blob_sha256)).collect();
        let known = names.iter().cloned().collect();
        let mut contacts = HashMap::new();
        contacts.insert("a".into(), names[..2].iter().cloned().collect());
        contacts.insert("b".into(), HashSet::from([names[2].clone()]));
        fs::remove_file(path(&dir, &photos[1].blob_sha256)).unwrap();
        fs::create_dir(path(&dir, &photos[1].blob_sha256)).unwrap();
        assert!(prune_contacts(&dir, &known, &contacts, 1).is_err());
        assert!(is_kept(&dir, &photos[2].blob_sha256));
        fs::remove_dir(path(&dir, &photos[1].blob_sha256)).unwrap();
        prune_contacts(&dir, &known, &contacts, u64::MAX).unwrap();
        assert!(is_kept(&dir, &photos[0].blob_sha256));
        fs::remove_dir_all(dir).unwrap();
    }
}
