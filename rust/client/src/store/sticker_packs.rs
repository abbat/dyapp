//! Saved packs use the photo sealing secret and identity-local files, never automatic eviction.

use super::Store;
use crate::{
    attachment,
    sticker_pack::{self, Pack},
};
use dyapp_identity::sha256;
use rusqlite::TransactionBehavior;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

/// Durable, bounded per-chat requests for packs selected from received stickers.
pub mod exchange;
/// Bounded collection metadata and per-entry read errors.
pub mod list;
/// Reusable sender descriptors and charged upload reservations.
pub mod uploads;

/// Maximum encrypted saved-file bytes for one identity, including file headers.
pub const SAVED_BYTES: u64 = 300 * 1024 * 1024;
const MAGIC: &[u8] = b"DYSP\x01";
const HEADER: usize = 37;
const MAX_FILE: usize = 6 * 1024 * 1024 + HEADER;

/// Why a saved pack cannot be read or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The fixed saved-pack quota would be exceeded.
    #[error("saved sticker pack quota exceeded")]
    Quota,
    /// Content, identity/key, file type or local format is invalid.
    #[error("saved sticker pack is invalid or cannot be opened")]
    Invalid,
    /// The filesystem failed.
    #[error("saved sticker pack: {0}")]
    Io(#[from] io::Error),
    /// A concurrent store writer or SQLite failure prevented the operation.
    #[error("saved sticker pack: {0}")]
    Sql(#[from] rusqlite::Error),
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn directory(store: &Store) -> Result<PathBuf, Error> {
    let parent = store
        .db
        .path()
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .and_then(Path::parent)
        .ok_or(Error::Invalid)?;
    Ok(parent.join("sticker-packs"))
}
fn valid_directory(dir: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() && !meta.is_symlink() => Ok(true),
        Ok(_) => Err(Error::Invalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn used(dir: &Path) -> Result<u64, Error> {
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let meta = entry?.path().symlink_metadata()?;
        if !meta.is_file() || meta.is_symlink() {
            return Err(Error::Invalid);
        }
        total = total.checked_add(meta.len()).ok_or(Error::Quota)?;
    }
    Ok(total)
}
fn key(secret: &[u8; 32], owner: &str, id: &[u8; 32], digest: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let input = Zeroizing::new(
        [
            b"dyapp-saved-sticker-pack-v1".as_slice(),
            secret,
            owner.as_bytes(),
            id,
            digest,
        ]
        .concat(),
    );
    Zeroizing::new(sha256(&input))
}
fn read(path: &Path, secret: &[u8; 32], owner: &str, id: &[u8; 32]) -> Result<Option<Pack>, Error> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.is_symlink() => {}
        Ok(_) => return Err(Error::Invalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_FILE + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() < HEADER || bytes.len() > MAX_FILE || !bytes.starts_with(MAGIC) {
        return Err(Error::Invalid);
    }
    let digest: [u8; 32] = bytes[MAGIC.len()..HEADER]
        .try_into()
        .map_err(|_| Error::Invalid)?;
    let plain = attachment::decrypt(&key(secret, owner, id, &digest), &bytes[HEADER..])
        .map_err(|_| Error::Invalid)?;
    if sha256(&plain) != digest {
        return Err(Error::Invalid);
    }
    sticker_pack::from_peer(&plain, id)
        .map(Some)
        .map_err(|_| Error::Invalid)
}

impl Store {
    /// Whether files already require the existing identity sealing secret, even if damaged.
    pub fn has_sticker_packs(&self) -> Result<bool, Error> {
        let dir = directory(self)?;
        if !valid_directory(&dir)? {
            return Ok(false);
        }
        Ok(fs::read_dir(dir)?.next().transpose()?.is_some())
    }
    /// Authenticates and revalidates one saved pack; corrupt/foreign files are never removed.
    pub fn saved_sticker_pack(
        &self,
        secret: &[u8; 32],
        id: &[u8; 32],
    ) -> Result<Option<Pack>, Error> {
        let dir = directory(self)?;
        if !valid_directory(&dir)? {
            return Ok(None);
        }
        read(&dir.join(hex(id)), secret, &self.owner_peer_id, id)
    }
    /// Removes only the explicitly selected pack, including corrupt content; missing is false.
    /// Callers must obtain user confirmation. An error after unlink may still mean removal
    /// happened, so refresh before retrying. This does not erase copies held elsewhere.
    pub fn remove_saved_sticker_pack(&mut self, id: &[u8; 32]) -> Result<bool, Error> {
        let dir = directory(self)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !valid_directory(&dir)? {
            return Ok(false);
        }
        let path = dir.join(hex(id));
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() && !meta.is_symlink() => {}
            Ok(_) => return Err(Error::Invalid),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        fs::remove_file(path)?;
        #[cfg(unix)]
        File::open(&dir)?.sync_all()?;
        tx.commit()?;
        Ok(true)
    }
    /// Saves a validated pack atomically. Returns false for an already saved, readable pack.
    pub fn save_sticker_pack(&mut self, secret: &[u8; 32], pack: &Pack) -> Result<bool, Error> {
        self.save_pack_with_limit(secret, pack, SAVED_BYTES)
    }
    fn save_pack_with_limit(
        &mut self,
        secret: &[u8; 32],
        pack: &Pack,
        limit: u64,
    ) -> Result<bool, Error> {
        let dir = directory(self)?;
        let owner = self.owner_peer_id.clone();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        match fs::create_dir(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        if !valid_directory(&dir)? {
            return Err(Error::Invalid);
        }
        let path = dir.join(hex(pack.id()));
        if read(&path, secret, &owner, pack.id())?.is_some() {
            return Ok(false);
        }
        let plain = pack.encode();
        let digest = sha256(&plain);
        let sealed = attachment::encrypt(&key(secret, &owner, pack.id(), &digest), &plain)
            .map_err(|_| Error::Invalid)?;
        let mut bytes = MAGIC.to_vec();
        bytes.extend(digest);
        bytes.extend(sealed);
        if used(&dir)?
            .checked_add(bytes.len() as u64)
            .is_none_or(|size| size > limit)
        {
            return Err(Error::Quota);
        }
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| io::Error::other("no OS randomness"))?;
        let pending = dir.join(format!(".{}.pending", hex(&random)));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        let published = (|| -> Result<(), Error> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&pending, &path)?;
            #[cfg(unix)]
            File::open(&dir)?.sync_all()?;
            Ok(())
        })();
        if published.is_err() {
            let _ = fs::remove_file(&pending);
        }
        published?;
        tx.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
