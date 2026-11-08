//! Bounded collection pages; filenames are never rendered as trusted display text.
use super::{directory, fs, hex, read, valid_directory, Error, Store};
use std::collections::BTreeMap;

const PAGE_SIZE: usize = 32;

/// Authenticated metadata for a saved pack.
pub struct Details {
    /// Display-safe manifest name.
    pub name: String,
    /// Number of validated stickers.
    pub stickers: usize,
}
/// One saved filename, including a recoverable unreadable-content result.
pub struct Entry {
    /// Manifest ID named by the file.
    pub id: [u8; 32],
    /// Actual encrypted file size, including the local header.
    pub bytes: u64,
    /// Metadata, or the error that prevented opening this entry.
    pub details: Result<Details, Error>,
}
/// A bounded view of a collection; concurrent saves can appear on later refresh.
pub struct Page {
    /// Up to 32 entries in ascending `PackID` order.
    pub entries: Vec<Entry>,
    /// Exclusive cursor for the next page, if more filenames were found.
    pub next: Option<[u8; 32]>,
    /// All regular file bytes, including interrupted writes.
    pub used_bytes: u64,
    /// Files without a canonical `PackID` name, including interrupted writes.
    pub unrecognized_files: u64,
}
fn parse_id(name: &std::ffi::OsStr) -> Option<[u8; 32]> {
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }
    let name = name.to_str()?.as_bytes();
    if name.len() != 64 {
        return None;
    }
    let mut id = [0; 32];
    for (index, pair) in name.as_chunks::<2>().0.iter().enumerate() {
        id[index] = nibble(pair[0])? * 16 + nibble(pair[1])?;
    }
    Some(id)
}
impl Store {
    /// Lists metadata without retaining compressed packs or decoded frames.
    /// Invalid content remains visible as an entry error; no file is removed.
    ///
    /// # Errors
    ///
    /// Returns an error if the store path or directory is invalid, a directory
    /// entry is not a regular file, filesystem enumeration or metadata fails,
    /// or summing file sizes overflows. Individual pack-read failures remain in
    /// each entry's `details` result.
    pub fn saved_sticker_packs(
        &self,
        secret: &[u8; 32],
        after: Option<&[u8; 32]>,
    ) -> Result<Page, Error> {
        let dir = directory(self)?;
        let mut page = Page {
            entries: Vec::new(),
            next: None,
            used_bytes: 0,
            unrecognized_files: 0,
        };
        if !valid_directory(&dir)? {
            return Ok(page);
        }
        let mut candidates = BTreeMap::new();
        let mut more = false;
        for file in fs::read_dir(&dir)? {
            let file = file?;
            let meta = file.path().symlink_metadata()?;
            if !meta.is_file() || meta.is_symlink() {
                return Err(Error::Invalid);
            }
            page.used_bytes = page
                .used_bytes
                .checked_add(meta.len())
                .ok_or(Error::Quota)?;
            let Some(id) = parse_id(&file.file_name()) else {
                page.unrecognized_files += 1;
                continue;
            };
            if after.is_some_and(|cursor| id <= *cursor) {
                continue;
            }
            candidates.insert(id, meta.len());
            if candidates.len() > PAGE_SIZE {
                candidates.pop_last();
                more = true;
            }
        }
        if more {
            page.next = candidates.last_key_value().map(|(id, _)| *id);
        }
        for (id, bytes) in candidates {
            let details = read(&dir.join(hex(&id)), secret, &self.owner_peer_id, &id)
                .and_then(|pack| pack.ok_or(Error::Invalid))
                .map(|pack| Details {
                    name: pack.name().to_owned(),
                    stickers: pack.stickers().len(),
                });
            page.entries.push(Entry { id, bytes, details });
        }
        Ok(page)
    }
}
