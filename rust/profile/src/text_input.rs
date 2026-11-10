//! Client input whitespace shared by profile parsers and higher-level client policy.

/// Trim the ECMAScript set used by desktop input: Unicode whitespace except NEL, plus BOM.
#[must_use]
pub fn trim(text: &str) -> &str {
    text.trim_matches(|ch: char| (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}')
}
