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

#[cfg(test)]
mod tests {
    use super::{seed_legacy_search_hide, Arc, IdentityKey, Store};

    #[test]
    fn seeds_only_an_existing_empty_slot_with_the_legacy_review() {
        let dir = dyapp_test_support::test_dir().unwrap();
        let directory = dir.path().to_str().unwrap().to_owned();
        let key = IdentityKey::generate().unwrap();
        assert!(seed_legacy_search_hide(directory.clone(), Arc::clone(&key)).is_err());
        let store = Store::open(&dir.path().join("client.db"), &key.identity).unwrap();
        seed_legacy_search_hide(directory.clone(), Arc::clone(&key)).unwrap();
        let draft = store.profile_draft().unwrap().unwrap();
        assert_eq!(draft.version, 2);
        assert_eq!(draft.name, "Original private draft");
        assert!(store.search_visibility().unwrap().hidden);
        assert!(store.prepare_search_withdrawal_recovery().unwrap().is_some());
        assert!(seed_legacy_search_hide(directory, key).is_err());
    }
}
