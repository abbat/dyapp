//! Read-only eligibility for publishing the owner's stored profile draft.

use crate::{load_profile_draft, CoreError, IdentityKey};
use dyapp_client::publish::{self, Block};
use dyapp_profile::{CivilDate, Profile};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn today() -> Result<CivilDate, CoreError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Failed(error.to_string()))?;
    CivilDate::from_unix_days(now.as_secs() / 86_400)
        .ok_or_else(|| CoreError::Failed("invalid UTC date".into()))
}

fn block_at(profile: &Profile, attested: bool, today: CivilDate) -> Option<&'static str> {
    publish::check(profile, attested, today)
        .err()
        .map(|block| match block {
            Block::NotAttested => "notAttested",
            Block::InvalidBirthDate => "invalidBirthDate",
            Block::Underage => "underage",
        })
}

/// Return a stable blocking reason, or none if this identity's saved draft is publishable today.
/// Attestation is supplied fresh for each attempt; this read never creates a draft or database.
#[uniffi::export]
pub fn profile_publish_block(
    dir: String,
    key: Arc<IdentityKey>,
    attested: bool,
) -> Result<Option<String>, CoreError> {
    let mut profile = Profile::default();
    if let Some(draft) = load_profile_draft(dir, key)? {
        draft.apply_to(&mut profile);
    }
    Ok(block_at(&profile, attested, today()?).map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn empty_profile_needs_only_fresh_attestation_without_creating_a_database() {
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = IdentityKey::generate();
        let dir = dir.to_str().unwrap().to_owned();
        assert_eq!(
            profile_publish_block(dir.clone(), key.clone(), false).unwrap(),
            Some("notAttested".into())
        );
        assert_eq!(profile_publish_block(dir.clone(), key, true).unwrap(), None);
        assert!(!Path::new(&dir).join("client.db").exists());
    }

    #[test]
    fn yearless_parts_need_no_age_guess_but_an_underage_year_blocks() {
        let today = CivilDate::new(2025, 1, 1).unwrap();
        let yearless = Profile {
            birth_month: Some(2),
            birth_day: Some(29),
            ..Profile::default()
        };
        assert_eq!(block_at(&yearless, true, today), None);
        let underage = Profile {
            birth_year: Some(2010),
            ..Profile::default()
        };
        assert_eq!(block_at(&underage, true, today), Some("underage"));
        assert_eq!(block_at(&underage, false, today), Some("notAttested"));
        let impossible = Profile {
            birth_year: Some(1900),
            birth_month: Some(2),
            birth_day: Some(29),
            ..Profile::default()
        };
        assert_eq!(block_at(&impossible, true, today), Some("invalidBirthDate"));
    }
}
