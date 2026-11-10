//! Shared interpretation of text entered in clients using the desktop trim contract.

/// ECMAScript differs from Unicode whitespace by excluding NEL and including BOM.
pub(crate) fn trim(text: &str) -> &str {
    text.trim_matches(|ch: char| (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}')
}
