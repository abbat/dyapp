//! Common native adapter for profile editor birth fields.

use dyapp_profile::birth_input::{self, BirthPartValues};

/// Parse raw optional birth fields with the shared profile core; invalid input returns `None`.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned native strings"
)]
#[must_use]
pub fn parse_profile_birth_parts(
    year: String,
    month: String,
    day: String,
) -> Option<BirthPartValues> {
    birth_input::parse_birth_parts(&year, &month, &day)
}

/// Validate birth field values using the shared published-profile validity rule.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned native maps"
)]
#[must_use]
pub fn valid_profile_birth_parts(values: BirthPartValues) -> bool {
    birth_input::valid_birth_parts(&values)
}
