//! Resolve recent hash-plus-label choices against saved packs, retaining at most one pack.
use super::{thumbnail, Choice, PAGE_SIZE};
use crate::{
    sticker_pack::Pack,
    store::{
        sticker_packs::{recents::RecentSticker, Error as PackError},
        Store,
    },
};

/// Failed recent lookup, decoding or cancelled identity ownership.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid cursor or thumbnail/content failure.
    #[error(transparent)]
    Picker(#[from] super::Error),
    /// Persistent recent metadata failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    /// Saved-pack access failed.
    #[error(transparent)]
    Pack(#[from] PackError),
    /// The adapter's identity transition cancelled this page.
    #[error("Identity changed")]
    IdentityChanged,
}

/// A resolved recent entry with an authenticated pack name and selected label.
pub struct Entry {
    /// Immutable manifest identity.
    pub pack_id: [u8; 32],
    /// Validated saved-pack name.
    pub pack_name: String,
    /// Exact manifest selection and bounded pixels.
    pub sticker: Choice,
}

/// Filtered recent choices with a cursor over the original bounded recent list.
pub struct Page {
    /// At most eight available choices.
    pub entries: Vec<Entry>,
    /// Original-list offset of the next available entry, not filtered-list length.
    pub next: Option<usize>,
}

fn available_pack(store: &Store, seal: &[u8; 32], id: &[u8; 32]) -> Result<Option<Pack>, Error> {
    match store.saved_sticker_pack(seal, id) {
        Ok(pack) => Ok(pack),
        Err(PackError::Invalid) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

struct Cache<F> {
    load: F,
    id: Option<[u8; 32]>,
    pack: Option<Pack>,
}
impl<F: FnMut(&[u8; 32]) -> Result<Option<Pack>, Error>> Cache<F> {
    fn get(&mut self, id: &[u8; 32]) -> Result<Option<&Pack>, Error> {
        if self.id != Some(*id) {
            drop(self.pack.take());
            self.pack = (self.load)(id)?;
            self.id = Some(*id);
        }
        if self.pack.as_ref().is_some_and(|pack| pack.id() != id) {
            return Err(super::Error::Corrupted.into());
        }
        Ok(self.pack.as_ref())
    }
}

fn check(stop: &impl Fn() -> bool) -> Result<(), Error> {
    if stop() {
        return Err(Error::IdentityChanged);
    }
    Ok(())
}

fn resolve(
    recent: &[RecentSticker],
    after: usize,
    load: impl FnMut(&[u8; 32]) -> Result<Option<Pack>, Error>,
    stop: impl Fn() -> bool,
) -> Result<Page, Error> {
    if after > recent.len() {
        return Err(super::Error::InvalidIndex.into());
    }
    check(&stop)?;
    let mut cache = Cache {
        load,
        id: None,
        pack: None,
    };
    let mut entries = Vec::new();
    let mut next = None;
    for (offset, record) in recent.iter().enumerate().skip(after) {
        check(&stop)?;
        let Some(pack) = cache.get(&record.pack_id)? else {
            continue;
        };
        check(&stop)?;
        let Some(index) = pack
            .stickers()
            .iter()
            .position(|entry| entry.sha256 == record.sha256 && entry.emoji == record.emoji)
        else {
            continue;
        };
        if entries.len() == PAGE_SIZE {
            next = Some(offset);
            break;
        }
        entries.push(Entry {
            pack_id: record.pack_id,
            pack_name: pack.name().into(),
            sticker: thumbnail(pack, index)?,
        });
    }
    check(&stop)?;
    Ok(Page { entries, next })
}

/// Read available recent choices; missing/invalid saved files and missing labels are filtered.
/// Pixels, order, exact hash-plus-emoji matching and pagination are decided here, not in adapters.
/// The caller retains identity admission and supplies its transition/cancellation signal.
///
/// # Errors
/// Returns invalid cursor, storage/decode failure or cancellation, never a partial page.
/// Invalid saved packs are skipped without deleting their source files.
pub fn page(
    store: &Store,
    seal: &[u8; 32],
    after: usize,
    stop: impl Fn() -> bool,
) -> Result<Page, Error> {
    let recent = store.recent_stickers()?;
    resolve(&recent, after, |id| available_pack(store, seal, id), stop)
}

#[cfg(test)]
mod tests;
