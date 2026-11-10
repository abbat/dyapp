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
    /// ISO 3166-1 alpha-2 code from `place_countries`, empty when unset.
    pub country: String,
    /// Decimal `GeoNames` ID of a place in `country`, empty when unset.
    pub place: String,
    /// Optional public name, empty when unset; saving trims it.
    /// Last, so positional constructors keep working.
    #[uniffi(default = "")]
    pub name: String,
    /// Optional public height in whole centimetres, 50 to 272.
    #[uniffi(default = None)]
    pub height: Option<u32>,
    /// Optional plain text about the owner, at most 500 characters; saving trims it.
    #[uniffi(default = "")]
    pub about: String,
    /// Smoking habit.
    #[uniffi(default = 0)]
    pub smoking: u32,
    /// Drinking habit.
    #[uniffi(default = 0)]
    pub drinking: u32,
    /// Tattoo visibility.
    #[uniffi(default = 0)]
    pub tattoos: u32,
    /// Marital status.
    #[uniffi(default = 0)]
    pub marital_status: u32,
    /// Pets.
    #[uniffi(default = 0)]
    pub pets: u32,
    /// Gender codes the owner is looking for.
    #[uniffi(default = [])]
    pub seeking_genders: Vec<u32>,
    /// Lowest partner age, 18 to 120.
    #[uniffi(default = None)]
    pub seeking_age_from: Option<u32>,
    /// Highest partner age, 18 to 120.
    #[uniffi(default = None)]
    pub seeking_age_to: Option<u32>,
    /// Not looking now; saving drops the three fields above.
    #[uniffi(default = false)]
    pub not_looking: bool,
}

impl From<&Profile> for PublicProfileDraft {
    fn from(profile: &Profile) -> Self {
        Self {
            name: profile.name.clone(),
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
            country: profile.country.clone(),
            place: profile.place.clone(),
            height: profile.height,
            about: profile.about.clone(),
            smoking: profile.smoking,
            drinking: profile.drinking,
            tattoos: profile.tattoos,
            marital_status: profile.marital_status,
            pets: profile.pets,
            seeking_genders: profile.seeking_genders.clone(),
            seeking_age_from: profile.seeking_age_from,
            seeking_age_to: profile.seeking_age_to,
            not_looking: profile.not_looking,
        }
    }
}

impl PublicProfileDraft {
    fn validate(&self) -> Result<(), CoreError> {
        let name = self.name.trim();
        if name.chars().count() > dyapp_profile::MAX_FIELD_CHARS
            || !dyapp_profile::is_clean_text(name)
        {
            return Err(CoreError::Failed("invalidName".into()));
        }
        if !dyapp_profile::valid_height(self.height) {
            return Err(CoreError::Failed("invalidHeight".into()));
        }
        if !dyapp_profile::valid_about(self.about.trim()) {
            return Err(CoreError::Failed("invalidAbout".into()));
        }
        if !dyapp_profile::editor_input::valid_editor_seeking(
            &self.seeking_genders,
            self.seeking_age_from,
            self.seeking_age_to,
        ) {
            return Err(CoreError::Failed("invalidSeeking".into()));
        }
        self.validate_scales()?;
        let birth = Profile {
            birth_year: self.birth_year,
            birth_month: self.birth_month,
            birth_day: self.birth_day,
            ..Profile::default()
        };
        if !birth.valid_birth_parts() {
            return Err(CoreError::Failed("invalid birth date".into()));
        }
        if !dyapp_client::places::known(&self.country, &self.place) {
            return Err(CoreError::Failed("invalid place".into()));
        }
        Ok(())
    }

    fn validate_scales(&self) -> Result<(), CoreError> {
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
            (dyapp_profile::Smoking::FIELD, self.smoking),
            (dyapp_profile::Drinking::FIELD, self.drinking),
            (dyapp_profile::Tattoos::FIELD, self.tattoos),
            (dyapp_profile::MaritalStatus::FIELD, self.marital_status),
            (dyapp_profile::Pets::FIELD, self.pets),
        ] {
            let scale = dyapp_profile::scale(field)
                .ok_or_else(|| CoreError::Failed("unknown profile scale".into()))?;
            if !scale.knows(value) {
                return Err(CoreError::Failed("invalid profile scale".into()));
            }
        }
        Ok(())
    }

    pub(crate) fn apply_to(&self, profile: &mut Profile) {
        self.name.trim().clone_into(&mut profile.name);
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
        profile.country.clone_from(&self.country);
        profile.place.clone_from(&self.place);
        profile.height = self.height;
        self.about.trim().clone_into(&mut profile.about);
        profile.smoking = self.smoking;
        profile.drinking = self.drinking;
        profile.tattoos = self.tattoos;
        profile.marital_status = self.marital_status;
        profile.pets = self.pets;
        profile.not_looking = self.not_looking;
        if self.not_looking {
            profile.seeking_genders.clear();
            profile.seeking_age_from = None;
            profile.seeking_age_to = None;
        } else {
            profile.seeking_genders.clone_from(&self.seeking_genders);
            profile.seeking_age_from = self.seeking_age_from;
            profile.seeking_age_to = self.seeking_age_to;
        }
    }
}

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

pub(crate) fn database(dir: &str) -> Result<&Path, CoreError> {
    let dir = Path::new(dir);
    if !dir.is_absolute() || !dir.is_dir() || dir.is_symlink() {
        return Err(CoreError::Failed("invalid identity directory".into()));
    }
    Ok(dir)
}

/// Load this identity's client.db draft; absence does not create a database or a profile.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if the identity directory or database is invalid or cannot
/// be read, or the saved draft format or protobuf is invalid.
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
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
///
/// # Errors
///
/// Returns an error if editable fields fail validation, the identity directory
/// or database cannot be opened, or reading or saving the draft fails.
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
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
            country: "DE".into(),
            place: "2950159".into(),
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
    fn height_is_saved_cleared_and_refused_out_of_range() {
        let dir = temp_dir();
        let owner = IdentityKey::generate();
        let save = |height| {
            let draft = PublicProfileDraft {
                height,
                ..Default::default()
            };
            save_profile_draft(dir.clone(), owner.clone(), draft)
        };
        let saved = || {
            load_profile_draft(dir.clone(), owner.clone())
                .unwrap()
                .unwrap()
                .height
        };
        save(Some(180)).unwrap();
        assert_eq!(saved(), Some(180));
        for height in [49, 273] {
            assert!(save(Some(height)).is_err(), "{height}");
        }
        assert_eq!(saved(), Some(180));
        save(None).unwrap();
        assert_eq!(saved(), None);
    }

    #[test]
    fn seeking_round_trips_refuses_bad_values_and_not_looking_drops_it() {
        let dir = temp_dir();
        let owner = IdentityKey::generate();
        let woman = dyapp_profile::Gender::Woman.code();
        let seeking = PublicProfileDraft {
            seeking_genders: vec![woman],
            seeking_age_from: Some(25),
            seeking_age_to: Some(40),
            ..Default::default()
        };
        save_profile_draft(dir.clone(), owner.clone(), seeking.clone()).unwrap();
        let saved = || load_profile_draft(dir.clone(), owner.clone()).unwrap();
        assert_eq!(saved(), Some(seeking.clone()));
        for refused in [
            PublicProfileDraft {
                seeking_genders: vec![999],
                ..Default::default()
            },
            PublicProfileDraft {
                seeking_age_from: Some(41),
                seeking_age_to: Some(40),
                ..Default::default()
            },
        ] {
            let error = save_profile_draft(dir.clone(), owner.clone(), refused);
            assert!(matches!(error, Err(CoreError::Failed(code)) if code == "invalidSeeking"));
        }
        let not_looking = PublicProfileDraft {
            not_looking: true,
            ..seeking
        };
        save_profile_draft(dir.clone(), owner.clone(), not_looking).unwrap();
        let expected = PublicProfileDraft {
            not_looking: true,
            ..Default::default()
        };
        assert_eq!(saved(), Some(expected));
    }

    #[test]
    fn the_new_fields_round_trip_and_about_is_trimmed_and_limited() {
        let dir = temp_dir();
        let owner = IdentityKey::generate();
        let draft = PublicProfileDraft {
            about: " First line\nsecond ".into(),
            smoking: dyapp_profile::Smoking::Sometimes.code(),
            drinking: dyapp_profile::Drinking::No.code(),
            tattoos: dyapp_profile::Tattoos::Visible.code(),
            marital_status: dyapp_profile::MaritalStatus::Divorced.code(),
            pets: dyapp_profile::Pets::Yes.code(),
            ..Default::default()
        };
        save_profile_draft(dir.clone(), owner.clone(), draft.clone()).unwrap();
        let saved = load_profile_draft(dir.clone(), owner.clone()).unwrap();
        let expected = PublicProfileDraft {
            about: "First line\nsecond".into(),
            ..draft.clone()
        };
        assert_eq!(saved, Some(expected));
        let about = |about: String| PublicProfileDraft {
            about,
            ..Default::default()
        };
        let longest = "я".repeat(dyapp_profile::MAX_ABOUT_CHARS);
        save_profile_draft(dir.clone(), owner.clone(), about(longest.clone())).unwrap();
        let error = save_profile_draft(dir.clone(), owner.clone(), about(longest + "я"));
        assert!(matches!(error, Err(CoreError::Failed(code)) if code == "invalidAbout"));
        let gap = PublicProfileDraft {
            smoking: 15,
            ..Default::default()
        };
        assert!(save_profile_draft(dir.clone(), owner.clone(), gap).is_err());
        save_profile_draft(dir.clone(), owner.clone(), PublicProfileDraft::default()).unwrap();
        let cleared = load_profile_draft(dir, owner).unwrap();
        assert_eq!(cleared, Some(PublicProfileDraft::default()));
    }

    #[test]
    fn a_public_name_is_trimmed_and_refused_with_hidden_symbols_or_too_long() {
        let dir = temp_dir();
        let owner = IdentityKey::generate();
        let named = |name: String| PublicProfileDraft {
            name,
            ..Default::default()
        };
        save_profile_draft(dir.clone(), owner.clone(), named("  Anna  ".into())).unwrap();
        let saved = load_profile_draft(dir.clone(), owner.clone()).unwrap();
        assert_eq!(saved.unwrap().name, "Anna");
        let long = "x".repeat(dyapp_profile::MAX_FIELD_CHARS + 1);
        for name in [long, "An\u{202E}na".into()] {
            let error = save_profile_draft(dir.clone(), owner.clone(), named(name));
            assert!(matches!(error, Err(CoreError::Failed(code)) if code == "invalidName"));
        }
        let saved = load_profile_draft(dir, owner).unwrap();
        assert_eq!(saved.unwrap().name, "Anna");
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
        let between = PublicProfileDraft {
            career_ambition: 15,
            ..Default::default()
        };
        assert!(save_profile_draft(dir.clone(), owner.clone(), between).is_err());
        let impossible = PublicProfileDraft {
            birth_year: Some(1900),
            birth_month: Some(2),
            birth_day: Some(29),
            ..Default::default()
        };
        assert!(save_profile_draft(dir.clone(), owner.clone(), impossible).is_err());
        for (country, place) in [
            ("XX", ""),
            ("", "2950159"),
            ("FR", "2950159"),
            ("DE", "Berlin"),
        ] {
            let unknown = PublicProfileDraft {
                country: country.into(),
                place: place.into(),
                ..Default::default()
            };
            assert!(save_profile_draft(dir.clone(), owner.clone(), unknown).is_err());
        }
        assert_eq!(
            load_profile_draft(dir, owner).unwrap(),
            Some(PublicProfileDraft::default())
        );
    }
}
