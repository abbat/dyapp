//! Client input whitespace shared by profile parsers and higher-level client policy.

/// Whether a character belongs to the ECMAScript input whitespace set.
#[must_use]
pub fn is_whitespace(ch: char) -> bool {
    (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}'
}

/// Trim the ECMAScript set used by desktop input: Unicode whitespace except NEL, plus BOM.
#[must_use]
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_whitespace)
}
