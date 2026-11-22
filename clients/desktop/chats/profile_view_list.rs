//! Read-only received-view pages; common Rust owns moderation, dates, matching and paging.
use dyapp_client::local_collection as core;
use dyapp_client::store::Store;

/// Wire cursor only; the common pager validates its query and clamps its offset.
#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct Cursor {
    query: String,
    offset: usize,
}

impl From<Cursor> for core::Cursor {
    fn from(value: Cursor) -> Self {
        Self {
            query: value.query,
            offset: value.offset,
        }
    }
}

impl From<core::Cursor> for Cursor {
    fn from(value: core::Cursor) -> Self {
        Self {
            query: value.query,
            offset: value.offset,
        }
    }
}

#[derive(serde::Serialize)]
struct Entry {
    target: String,
    label: String,
}

/// Bounded page DTO. Entry labels are core-formatted UTC dates, not profile names or exact times.
#[derive(serde::Serialize)]
pub(crate) struct Page {
    total: usize,
    matched: usize,
    number: usize,
    pages: usize,
    entries: Vec<Entry>,
    previous: Option<Cursor>,
    next: Option<Cursor>,
}

impl From<core::Page> for Page {
    fn from(page: core::Page) -> Self {
        Self {
            total: page.total,
            matched: page.matched,
            number: page.number,
            pages: page.pages,
            entries: page
                .entries
                .into_iter()
                .map(|entry| Entry {
                    target: entry.target,
                    label: entry.label,
                })
                .collect(),
            previous: page.previous.map(Into::into),
            next: page.next.map(Into::into),
        }
    }
}

fn page(store: &Store, query: &str, cursor: Option<&core::Cursor>) -> Result<Page, String> {
    let entries: Vec<_> = store
        .received_profile_views()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|view| core::Entry {
            target: view.peer_id,
            label: view.utc_date,
        })
        .collect();
    Ok(core::page(&entries, query, cursor).into())
}

/// Local history only, independent of the send preference. Never fetches profiles or sends signals.
#[tauri::command]
pub(crate) async fn received_profile_views(
    state: tauri::State<'_, crate::IdentityState>,
    query: String,
    cursor: Option<Cursor>,
) -> Result<Page, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        page(
            &super::open(&dir, &identity)?,
            &query,
            cursor.map(Into::into).as_ref(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
#[path = "profile_view_list/tests.rs"]
mod tests;
