//! Common native conversion layer for shared profile editor input.

use dyapp_profile::editor_input;
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
