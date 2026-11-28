//! Fixture setup for isolated native iOS UI tests.

use crate::{CoreError, IdentityKey};
use dyapp_client::store::Store;
use dyapp_profile::Profile;
use std::{path::Path, sync::Arc};

/// Populate a fresh test slot with a hidden draft and an old pending withdrawal.
///
/// # Errors
/// Returns an error if the test slot already has a draft or fixture storage fails.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn seed_legacy_search_hide(dir: String, key: Arc<IdentityKey>) -> Result<(), CoreError> {
    let database = Path::new(&dir).join("client.db");
    let owner = &key.identity;
    let store = Store::open_existing(&database, owner).map_err(failed)?;
    if store.profile_draft().map_err(failed)?.is_some() {
        return Err(CoreError::Failed("Expected a fresh fixture slot".into()));
    }
    store
        .save_profile_draft(&Profile {
            version: 2,
            name: "Original private draft".into(),
            ..Default::default()
        })
        .map_err(failed)?;
    store.set_profile_hidden(true).map_err(failed)?;
    store
        .hold_search_profile(&Profile::tombstone(2, 0, 0).sign(owner))
        .map_err(failed)
}

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}
