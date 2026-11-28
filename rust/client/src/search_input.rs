//! Presentation-independent interpretation of raw search form input.

use crate::search::Employer;
use crate::text_input::trim;

mod choices;
pub use choices::SearchChoiceInputs;
mod ranges;
pub use ranges::range_conditions;
mod visibility;
pub use visibility::visible_filter_fields;

/// Build node conditions in range-first order using the requested UTC date.
///
/// # Errors
/// Reports the first invalid offered range or interest.
pub fn filter_conditions(
    offered: &[u32],
    ranges: &dyapp_profile::search_input::SearchRangeInputs,
    choices: &SearchChoiceInputs,
    date: dyapp_profile::CivilDate,
) -> Result<Vec<dyapp_p2p_net::proto::Condition>, dyapp_profile::search_input::SearchInputError> {
    use dyapp_profile::search_input::SearchInputError;

    let mut conditions = range_conditions(offered, ranges, date)?;
    conditions.extend(
        choices
            .checked_conditions(offered)
            .map_err(|_| SearchInputError::Interest)?,
    );
    Ok(conditions)
}

/// The selected local employer-filter mode before interpreting its text field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmployerMode {
    /// Ignore company text and retain every employer.
    #[default]
    Any,
    /// Match one literal company name.
    Same,
    /// Exclude semicolon-separated company names.
    Exclude,
}

/// Interpret company text using the Linux desktop trim and separator contract.
///
/// Blank text means any employer. Same preserves semicolons; Exclude drops empty entries
/// while preserving spelling, order and duplicates. Matching remains in [`Employer::retain`].
#[must_use]
pub fn employer_filter(mode: EmployerMode, input: &str) -> Employer {
    let text = trim(input);
    match (mode, text.is_empty()) {
        (EmployerMode::Any, _) | (_, true) => Employer::Any,
        (EmployerMode::Same, false) => Employer::Same(text.into()),
        (EmployerMode::Exclude, false) => Employer::Exclude(
            text.split(';')
                .map(trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests;
