//! Closed editor validation and normalization, distinct from forward-compatible node verification.

use dyapp_profile::{self as profile, Profile};

/// Stable editable-draft failure, before any storage mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DraftError {
    /// Invalid public name after trimming.
    #[error("invalidName")]
    Name,
    /// Height outside the shared bounds.
    #[error("invalidHeight")]
    Height,
    /// Invalid about text after trimming.
    #[error("invalidAbout")]
    About,
    /// Invalid editor seeking choices or ages.
    #[error("invalidSeeking")]
    Seeking,
    /// An editor field has no vocabulary.
    #[error("unknown profile scale")]
    UnknownScale,
    /// A categorical code is not in the closed editor vocabulary.
    #[error("invalid profile scale")]
    Scale,
    /// Impossible partial birth date.
    #[error("invalid birth date")]
    Birth,
    /// Unknown country/place combination.
    #[error("invalid place")]
    Place,
}

/// Validate editable fields only; protocol/proof/device fields are outside this contract.
///
/// Unlike node verification, editor categorical choices must be known to this client.
///
/// # Errors
/// Returns the first invalid field in the established native draft validation order.
pub fn validate(draft: &Profile) -> Result<(), DraftError> {
    let name = draft.name.trim();
    if name.chars().count() > profile::MAX_FIELD_CHARS || !profile::is_clean_text(name) {
        return Err(DraftError::Name);
    }
    if !profile::valid_height(draft.height) {
        return Err(DraftError::Height);
    }
    if !profile::valid_about(draft.about.trim()) {
        return Err(DraftError::About);
    }
    if !profile::editor_input::valid_editor_seeking(
        &draft.seeking_genders,
        draft.seeking_age_from,
        draft.seeking_age_to,
    ) {
        return Err(DraftError::Seeking);
    }
    validate_scales(draft)?;
    if !draft.valid_birth_parts() {
        return Err(DraftError::Birth);
    }
    if !crate::places::known(&draft.country, &draft.place) {
        return Err(DraftError::Place);
    }
    Ok(())
}

fn validate_scales(draft: &Profile) -> Result<(), DraftError> {
    for (field, value) in [
        (profile::Gender::FIELD, draft.gender),
        (profile::Orientation::FIELD, draft.orientation),
        (profile::RelationshipGoal::FIELD, draft.relationship_goal),
        (profile::RolePreference::FIELD, draft.role_preference),
        (profile::CareerAmbition::FIELD, draft.career_ambition),
        (
            profile::FinancialPhilosophy::FIELD,
            draft.financial_philosophy,
        ),
        (profile::EmploymentStatus::FIELD, draft.employment_status),
        (profile::FitnessLevel::FIELD, draft.fitness_level),
        (profile::Zodiac::FIELD, draft.zodiac),
        (profile::Smoking::FIELD, draft.smoking),
        (profile::Drinking::FIELD, draft.drinking),
        (profile::Tattoos::FIELD, draft.tattoos),
        (profile::MaritalStatus::FIELD, draft.marital_status),
        (profile::Pets::FIELD, draft.pets),
    ] {
        let scale = profile::scale(field).ok_or(DraftError::UnknownScale)?;
        if !scale.knows(value) {
            return Err(DraftError::Scale);
        }
    }
    Ok(())
}

/// Apply normalized editable fields, preserving every other field in the target.
///
/// This also serves read-only eligibility/search projections. Call validate before
/// persisting editor input; application alone deliberately does not validate it.
pub fn apply(draft: &Profile, target: &mut Profile) {
    draft.name.trim().clone_into(&mut target.name);
    target.gender = draft.gender;
    target.orientation = draft.orientation;
    target.relationship_goal = draft.relationship_goal;
    target.role_preference = draft.role_preference;
    target.career_ambition = draft.career_ambition;
    target.financial_philosophy = draft.financial_philosophy;
    target.employment_status = draft.employment_status;
    target.fitness_level = draft.fitness_level;
    target.zodiac = draft.zodiac;
    target.birth_year = draft.birth_year;
    target.birth_month = draft.birth_month;
    target.birth_day = draft.birth_day;
    target.country.clone_from(&draft.country);
    target.place.clone_from(&draft.place);
    target.height = draft.height;
    draft.about.trim().clone_into(&mut target.about);
    target.smoking = draft.smoking;
    target.drinking = draft.drinking;
    target.tattoos = draft.tattoos;
    target.marital_status = draft.marital_status;
    target.pets = draft.pets;
    target.not_looking = draft.not_looking;
    if draft.not_looking {
        target.seeking_genders.clear();
        target.seeking_age_from = None;
        target.seeking_age_to = None;
    } else {
        target.seeking_genders.clone_from(&draft.seeking_genders);
        target.seeking_age_from = draft.seeking_age_from;
        target.seeking_age_to = draft.seeking_age_to;
    }
}

#[cfg(test)]
mod tests;
