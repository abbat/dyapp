//! Identity-bound native access to the shared unpublished profile draft.

use crate::{CoreError, IdentityKey};
use dyapp_client::store::Store;
use dyapp_profile::{self, Profile};
use std::path::Path;
use std::sync::Arc;

/// Editable public profile fields. Zero leaves any categorical field unpublished.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct PublicProfileDraft {
    /// Self-declared gender.
    pub gender: u32,
    /// Self-declared orientation.
    pub orientation: u32,
    /// Desired relationship.
    pub relationship_goal: u32,
    /// Preferred role.
    pub role_preference: u32,
    /// Career ambition.
    pub career_ambition: u32,
    /// Financial philosophy.
    pub financial_philosophy: u32,
    /// Employment status.
    pub employment_status: u32,
    /// Fitness level.
    pub fitness_level: u32,
    /// Zodiac sign.
    pub zodiac: u32,
    /// Optional public birth year.
    pub birth_year: Option<u32>,
    /// Optional public birth month.
    pub birth_month: Option<u32>,
    /// Optional public birth day.
    pub birth_day: Option<u32>,
}

impl From<&Profile> for PublicProfileDraft {
    fn from(profile: &Profile) -> Self {
        Self {
            gender: profile.gender,
            orientation: profile.orientation,
            relationship_goal: profile.relationship_goal,
            role_preference: profile.role_preference,
            career_ambition: profile.career_ambition,
            financial_philosophy: profile.financial_philosophy,
            employment_status: profile.employment_status,
            fitness_level: profile.fitness_level,
            zodiac: profile.zodiac,
            birth_year: profile.birth_year,
            birth_month: profile.birth_month,
            birth_day: profile.birth_day,
        }
    }
}

impl PublicProfileDraft {
    fn validate(&self) -> Result<(), CoreError> {
        for (field, value) in [
            (dyapp_profile::Gender::FIELD, self.gender),
            (dyapp_profile::Orientation::FIELD, self.orientation),
            (
                dyapp_profile::RelationshipGoal::FIELD,
                self.relationship_goal,
            ),
            (dyapp_profile::RolePreference::FIELD, self.role_preference),
            (dyapp_profile::CareerAmbition::FIELD, self.career_ambition),
            (
                dyapp_profile::FinancialPhilosophy::FIELD,
                self.financial_philosophy,
            ),
            (
                dyapp_profile::EmploymentStatus::FIELD,
                self.employment_status,
            ),
            (dyapp_profile::FitnessLevel::FIELD, self.fitness_level),
            (dyapp_profile::Zodiac::FIELD, self.zodiac),
        ] {
            let scale = dyapp_profile::scale(field)
                .ok_or_else(|| CoreError::Failed("unknown profile scale".into()))?;
            if value > scale.max {
                return Err(CoreError::Failed("invalid profile scale".into()));
            }
        }
        let birth = Profile {
            birth_year: self.birth_year,
            birth_month: self.birth_month,
            birth_day: self.birth_day,
            ..Profile::default()
        };
        if !birth.valid_birth_parts() {
            return Err(CoreError::Failed("invalid birth date".into()));
        }
        Ok(())
    }

    pub(crate) fn apply_to(&self, profile: &mut Profile) {
        profile.gender = self.gender;
        profile.orientation = self.orientation;
        profile.relationship_goal = self.relationship_goal;
        profile.role_preference = self.role_preference;
        profile.career_ambition = self.career_ambition;
        profile.financial_philosophy = self.financial_philosophy;
        profile.employment_status = self.employment_status;
        profile.fitness_level = self.fitness_level;
        profile.zodiac = self.zodiac;
        profile.birth_year = self.birth_year;
        profile.birth_month = self.birth_month;
        profile.birth_day = self.birth_day;
    }
}

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

fn database(dir: &str) -> Result<&Path, CoreError> {
    let dir = Path::new(dir);
    if !dir.is_absolute() || !dir.is_dir() || dir.is_symlink() {
        return Err(CoreError::Failed("invalid identity directory".into()));
    }
    Ok(dir)
}

/// Load this identity's client.db draft; absence does not create a database or a profile.
#[uniffi::export]
pub fn load_profile_draft(
    dir: String,
    key: Arc<IdentityKey>,
) -> Result<Option<PublicProfileDraft>, CoreError> {
    let path = database(&dir)?.join("client.db");
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failed(error)),
        Ok(_) => {}
    }
    let store = Store::open(&path, &key.identity).map_err(failed)?;
    store
        .profile_draft()
        .map(|profile| profile.as_ref().map(PublicProfileDraft::from))
        .map_err(failed)
}

/// Save only editable fields, retaining version and proof-of-work progress in client.db.
#[uniffi::export]
pub fn save_profile_draft(
    dir: String,
    key: Arc<IdentityKey>,
    draft: PublicProfileDraft,
) -> Result<(), CoreError> {
    draft.validate()?;
    let path = database(&dir)?.join("client.db");
    let store = Store::open(&path, &key.identity).map_err(failed)?;
    let mut profile = store.profile_draft().map_err(failed)?.unwrap_or_default();
    draft.apply_to(&mut profile);
    store.save_profile_draft(&profile).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-profile-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_str().unwrap().to_owned()
    }

    #[test]
    fn draft_is_identity_bound_and_preserves_noneditable_progress() {
        let dir = temp_dir();
        let owner = IdentityKey::generate();
        let stranger = IdentityKey::generate();
        assert_eq!(
            load_profile_draft(dir.clone(), owner.clone()).unwrap(),
            None
        );
        let draft = PublicProfileDraft {
            birth_month: Some(2),
            birth_day: Some(29),
            ..Default::default()
        };
        save_profile_draft(dir.clone(), owner.clone(), draft.clone()).unwrap();
        assert_eq!(
            load_profile_draft(dir.clone(), owner.clone()).unwrap(),
            Some(draft.clone())
        );
        assert!(load_profile_draft(dir.clone(), stranger).is_err());

        let store = Store::open(&Path::new(&dir).join("client.db"), &owner.identity).unwrap();
        let mut profile = store.profile_draft().unwrap().unwrap();
        profile.version = 7;
        profile.pow_bits = 4;
        store.save_profile_draft(&profile).unwrap();
        let edited = PublicProfileDraft { gender: 1, ..draft };
        save_profile_draft(dir.clone(), owner.clone(), edited.clone()).unwrap();
        assert_eq!(
            load_profile_draft(dir.clone(), owner).unwrap(),
            Some(edited)
        );
        let saved = store.profile_draft().unwrap().unwrap();
        assert_eq!((saved.version, saved.pow_bits), (7, 4));
    }

    #[test]
    fn invalid_edit_does_not_replace_a_saved_draft() {
        let dir = temp_dir();
        let owner = IdentityKey::generate();
        save_profile_draft(dir.clone(), owner.clone(), PublicProfileDraft::default()).unwrap();
        let invalid = PublicProfileDraft {
            gender: u32::MAX,
            ..Default::default()
        };
        assert!(save_profile_draft(dir.clone(), owner.clone(), invalid).is_err());
        let impossible = PublicProfileDraft {
            birth_year: Some(1900),
            birth_month: Some(2),
            birth_day: Some(29),
            ..Default::default()
        };
        assert!(save_profile_draft(dir.clone(), owner.clone(), impossible).is_err());
        assert_eq!(
            load_profile_draft(dir, owner).unwrap(),
            Some(PublicProfileDraft::default())
        );
    }
}
