//! Wire projection only; shared Rust owns local collection matching and page bounds.
use dyapp_client::local_collection as core;

/// One unchanged local target and its optional local search label.
#[derive(uniffi::Record)]
pub struct LocalCollectionEntry {
    /// Complete identifier of the original local record.
    pub target: String,
    /// Local human-readable search label, empty when absent.
    pub label: String,
}

/// Shared-core navigation token, scoped to the query and caller's collection opening.
#[derive(uniffi::Record)]
pub struct LocalCollectionCursor {
    /// Core-normalized query that produced this cursor.
    pub query: String,
    /// Zero-based filtered offset; the core aligns and clamps it.
    pub offset: u64,
}

/// Bounded common-core projection of one local collection snapshot.
#[derive(uniffi::Record)]
pub struct LocalCollectionPage {
    /// All supplied snapshot entries, including those hidden by search.
    pub total: u64,
    /// Matching entries before selecting a page.
    pub matched: u64,
    /// One-based current page number.
    pub number: u64,
    /// Total pages; an empty result has one empty page.
    pub pages: u64,
    /// At most 32 unchanged local entries in source order.
    pub entries: Vec<LocalCollectionEntry>,
    /// Previous page cursor, absent on the first page.
    pub previous: Option<LocalCollectionCursor>,
    /// Next page cursor, absent on the last page.
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
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts an owned query across the native boundary"
)]
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
