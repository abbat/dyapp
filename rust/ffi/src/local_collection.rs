//! Wire projection only; shared Rust owns local collection matching and page bounds.
use dyapp_client::local_collection as core;

#[derive(uniffi::Record)]
pub struct LocalCollectionEntry {
    pub target: String,
    pub label: String,
}

#[derive(uniffi::Record)]
pub struct LocalCollectionCursor {
    pub query: String,
    pub offset: u64,
}

#[derive(uniffi::Record)]
pub struct LocalCollectionPage {
    pub total: u64,
    pub matched: u64,
    pub number: u64,
    pub pages: u64,
    pub entries: Vec<LocalCollectionEntry>,
    pub previous: Option<LocalCollectionCursor>,
    pub next: Option<LocalCollectionCursor>,
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

impl From<core::Cursor> for LocalCollectionCursor {
    fn from(value: core::Cursor) -> Self {
        Self {
            query: value.query,
            offset: count(value.offset),
        }
    }
}

/// Call with one authenticated local snapshot; retain its owner/opening until rendering/actions.
#[uniffi::export]
pub fn local_collection_page(
    entries: Vec<LocalCollectionEntry>,
    query: String,
    cursor: Option<LocalCollectionCursor>,
) -> LocalCollectionPage {
    let entries: Vec<_> = entries
        .into_iter()
        .map(|entry| core::Entry {
            target: entry.target,
            label: entry.label,
        })
        .collect();
    let cursor = cursor.map(|cursor| core::Cursor {
        query: cursor.query,
        offset: usize::try_from(cursor.offset).unwrap_or(usize::MAX),
    });
    let page = core::page(&entries, &query, cursor.as_ref());
    LocalCollectionPage {
        total: count(page.total),
        matched: count(page.matched),
        number: count(page.number),
        pages: count(page.pages),
        entries: page
            .entries
            .into_iter()
            .map(|entry| LocalCollectionEntry {
                target: entry.target,
                label: entry.label,
            })
            .collect(),
        previous: page.previous.map(Into::into),
        next: page.next.map(Into::into),
    }
}
