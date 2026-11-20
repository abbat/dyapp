//! Pure bounded presentation of local collections; callers retain snapshot and owner authority.

/// Maximum mounted rows in a local collection page.
pub const PAGE_SIZE: usize = 32;

/// A stable action target and an optional local human-readable search label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub target: String,
    pub label: String,
}

/// Navigation is bound to the normalized query that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub query: String,
    pub offset: usize,
}

/// Counts describe the complete input snapshot, not only the mounted rows.
#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    pub total: usize,
    pub matched: usize,
    pub offset: usize,
    pub number: usize,
    pub pages: usize,
    pub entries: Vec<Entry>,
    pub previous: Option<Cursor>,
    pub next: Option<Cursor>,
}

/// Search full targets and labels without changing action targets or source ordering.
///
/// A changed query resets navigation. Removal/refresh clamps stale cursors to the last page;
/// the caller must discard a cursor and snapshot when the collection's owner/opening changes.
#[must_use]
pub fn page(entries: &[Entry], query: &str, cursor: Option<&Cursor>) -> Page {
    let query = crate::text_input::trim(query).to_lowercase();
    let filtered: Vec<_> = entries
        .iter()
        .filter(|entry| {
            query.is_empty()
                || entry.target.to_lowercase().contains(&query)
                || entry.label.to_lowercase().contains(&query)
        })
        .collect();
    let matched = filtered.len();
    let last = (matched.saturating_sub(1) / PAGE_SIZE).saturating_mul(PAGE_SIZE);
    let requested = cursor
        .filter(|cursor| cursor.query == query)
        .map_or(0, |cursor| cursor.offset);
    let offset = (requested / PAGE_SIZE).saturating_mul(PAGE_SIZE).min(last);
    let cursor_at = |offset| Cursor {
        query: query.clone(),
        offset,
    };
    Page {
        total: entries.len(),
        matched,
        offset,
        number: (offset / PAGE_SIZE).saturating_add(1),
        pages: (matched.saturating_sub(1) / PAGE_SIZE).saturating_add(1),
        entries: filtered
            .into_iter()
            .skip(offset)
            .take(PAGE_SIZE)
            .cloned()
            .collect(),
        previous: offset.checked_sub(PAGE_SIZE).map(cursor_at),
        next: (offset.saturating_add(PAGE_SIZE) < matched)
            .then(|| cursor_at(offset.saturating_add(PAGE_SIZE))),
    }
}

#[cfg(test)]
#[path = "local_collection/tests.rs"]
mod tests;
