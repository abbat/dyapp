//! Read-only presence projections shared by foreground adapters; never a timer or sender.
use crate::store::{Error, Store};

/// Whether any locally known chat is eligible for a foreground presence.
///
/// Uses the same local clock as common session staging. A false result lets clients avoid
/// opening a node connection; a true result is only preflight, never authority to bypass the
/// staging-time audience, match, block and hourly checks.
///
/// # Errors
/// Returns storage, eligibility or clock errors without treating unknown state as permission.
pub fn due(store: &Store) -> Result<bool, Error> {
    due_at(store, crate::now_ms())
}

fn due_at(store: &Store, wall_ms: u64) -> Result<bool, Error> {
    for row in store.chat_rows()? {
        if store.draft_presence(&row.peer_id, wall_ms)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Current chat peers whose authenticated match presence is still fresh.
///
/// Clients display only this projection and refresh it after mail passes; they must not
/// reimplement expiry, match or block policy in Kotlin/JavaScript.
///
/// # Errors
/// Returns storage or eligibility errors rather than an artificial empty/fresh result.
pub fn active_peers(store: &Store) -> Result<Vec<String>, Error> {
    active_peers_at(store, crate::now_ms())
}

fn active_peers_at(store: &Store, wall_ms: u64) -> Result<Vec<String>, Error> {
    let mut active = Vec::new();
    for row in store.chat_rows()? {
        if store.active_recently(&row.peer_id, wall_ms)? {
            active.push(row.peer_id);
        }
    }
    Ok(active)
}

#[cfg(test)]
mod tests;
