//! Checks a profile draft must pass before each publish.

use dyapp_profile::{CivilDate, Profile};

/// Why a draft cannot be published yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Block {
    /// The owner has not confirmed being 18 or older for this publish.
    #[error("confirm you are 18 or older")]
    NotAttested,
    /// Gender is required.
    #[error("gender is required")]
    NoGender,
    /// The year, month and day parts describe no calendar date.
    #[error("invalid birth date")]
    InvalidBirthDate,
    /// With a year, the youngest possible age is unknown or under 18.
    #[error("the birth date gives an age under 18")]
    Underage,
}

/// Check `draft` at `today`, the current UTC civil date. `attested` is the owner's fresh 18+
/// confirmation for this publish; without a birth year it is the only age check.
pub fn check(draft: &Profile, attested: bool, today: CivilDate) -> Result<(), Block> {
    if !attested {
        return Err(Block::NotAttested);
    }
    if draft.gender.trim().is_empty() {
        return Err(Block::NoGender);
    }
    if !draft.valid_birth_parts() {
        return Err(Block::InvalidBirthDate);
    }
    if draft.birth_year.is_some()
        && !draft
            .possible_age_range(today)
            .is_some_and(|(youngest, _)| youngest >= 18)
    {
        return Err(Block::Underage);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check, Block};
    use dyapp_profile::{CivilDate, Profile};

    fn draft(year: Option<u32>, month: Option<u32>, day: Option<u32>) -> Profile {
        Profile {
            gender: "woman".into(),
            birth_year: year,
            birth_month: month,
            birth_day: day,
            ..Profile::default()
        }
    }

    #[test]
    fn gates_each_publish_on_attestation_gender_and_age() {
        let today = CivilDate::new(2026, 10, 26).unwrap();
        assert_eq!(check(&draft(None, None, None), true, today), Ok(()));
        assert_eq!(check(&draft(None, Some(2), Some(29)), true, today), Ok(()));
        assert_eq!(
            check(&draft(None, None, None), false, today),
            Err(Block::NotAttested)
        );
        let mut genderless = draft(None, None, None);
        genderless.gender = " ".into();
        assert_eq!(check(&genderless, true, today), Err(Block::NoGender));
        assert_eq!(
            check(&draft(None, Some(4), Some(31)), true, today),
            Err(Block::InvalidBirthDate)
        );
        // Turns 18 today, turns 18 tomorrow, and a year that may still be under 18.
        assert_eq!(
            check(&draft(Some(2008), Some(10), Some(26)), true, today),
            Ok(())
        );
        assert_eq!(
            check(&draft(Some(2008), Some(10), Some(27)), true, today),
            Err(Block::Underage)
        );
        assert_eq!(
            check(&draft(Some(2008), None, None), true, today),
            Err(Block::Underage)
        );
        assert_eq!(check(&draft(Some(2007), None, None), true, today), Ok(()));
        // A future year gives no age at all.
        assert_eq!(
            check(&draft(Some(2030), None, None), true, today),
            Err(Block::Underage)
        );
    }
}
