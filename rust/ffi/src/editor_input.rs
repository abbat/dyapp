//! Common native conversion layer for shared profile editor input.

use dyapp_profile::{editor_input, MAX_FIELD_CHARS, MAX_INTERESTS};
use std::collections::HashMap;

/// Parse optional height input using the shared profile core.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned native strings"
)]
pub fn parse_profile_height(typed: String) -> Option<HashMap<String, i32>> {
    editor_input::parse_height(&typed)
}

/// Shared interest-editor limits; adapters display these rather than copying numeric policy.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ProfileInterestLimits {
    /// Maximum nonempty tag lines, including duplicates.
    pub max_tags: u64,
    /// Maximum Unicode scalar values in each canonical tag.
    pub max_tag_chars: u64,
}

/// Current authoritative interest limits.
#[uniffi::export]
#[must_use]
pub fn profile_interest_limits() -> ProfileInterestLimits {
    ProfileInterestLimits {
        max_tags: MAX_INTERESTS as u64,
        max_tag_chars: MAX_FIELD_CHARS as u64,
    }
}

/// Parse one interest per nonempty line through the common profile editor.
/// None is invalid; an empty successful list explicitly clears tags.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned native strings"
)]
pub fn parse_profile_interests(typed: String) -> Option<Vec<String>> {
    dyapp_client::profile_editor::parse_interest_lines(&typed).ok()
}

/// Shared normalized seeking fields for native profile editors.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ParsedProfileSeeking {
    /// Known gender codes in ascending order.
    pub genders: Vec<u32>,
    /// Optional inclusive minimum partner age.
    pub age_from: Option<u32>,
    /// Optional inclusive maximum partner age.
    pub age_to: Option<u32>,
    /// True means all other seeking fields are cleared.
    pub not_looking: bool,
}

impl From<editor_input::Seeking> for ParsedProfileSeeking {
    fn from(value: editor_input::Seeking) -> Self {
        Self {
            genders: value.genders,
            age_from: value.age_from,
            age_to: value.age_to,
            not_looking: value.not_looking,
        }
    }
}

/// Interpret raw seeking fields using the shared core; invalid input returns `None`.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned native inputs"
)]
pub fn parse_profile_seeking(
    genders: Vec<u32>,
    from: String,
    to: String,
    not_looking: bool,
) -> Option<ParsedProfileSeeking> {
    editor_input::parse_seeking(&genders, &from, &to, not_looking).map(Into::into)
}

#[cfg(test)]
#[path = "editor_input/interests_tests.rs"]
mod interests_tests;
