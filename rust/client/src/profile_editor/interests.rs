//! One canonical interest-list boundary for native and desktop editor input.
use super::DraftError;
use dyapp_profile::{is_clean_text, normalize_interest, MAX_FIELD_CHARS, MAX_INTERESTS};

/// Trim, lowercase and NFC-normalize tags, preserving order and existing duplicate semantics.
///
/// # Errors
/// Refuses excess tags, empty normalized tags, overlong tags and hidden/control characters.
pub fn normalize_interests(tags: &[String]) -> Result<Vec<String>, DraftError> {
    if tags.len() > MAX_INTERESTS {
        return Err(DraftError::Interests);
    }
    tags.iter()
        .map(|tag| {
            let tag = normalize_interest(tag);
            if tag.is_empty() || tag.chars().count() > MAX_FIELD_CHARS || !is_clean_text(&tag) {
                return Err(DraftError::Interests);
            }
            Ok(tag)
        })
        .collect()
}

/// Parse one tag per nonempty line, using the same canonical rules as list-based editors.
///
/// Blank lines are presentation separators; punctuation and duplicate ordering remain intact.
///
/// # Errors
/// Refuses an invalid tag or excess nonempty lines without returning a partial list.
pub fn parse_interest_lines(typed: &str) -> Result<Vec<String>, DraftError> {
    let tags = typed
        .lines()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .take(MAX_INTERESTS.saturating_add(1))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    normalize_interests(&tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_tags_share_the_wire_canonical_form_without_reordering() {
        let tags = vec![
            "  Музыка  ".into(),
            " E\u{301}COLE ".into(),
            "музыка".into(),
        ];
        let tags = normalize_interests(&tags).unwrap();
        assert_eq!(tags, ["музыка", "école", "музыка"]);
        assert_eq!(normalize_interests(&[]).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn multiline_input_preserves_punctuation_unicode_and_duplicate_order() {
        let tags =
            parse_interest_lines(" Music, Art; Books \r\n \nE\u{301}COLE\nmusic, art; books\n")
                .unwrap();
        assert_eq!(tags, ["music, art; books", "école", "music, art; books"]);
        assert!(parse_interest_lines(" \r\n\t\n").unwrap().is_empty());
    }

    #[test]
    fn multiline_count_budget_ignores_blank_lines_but_not_duplicates() {
        let limit = "tag\n\n".repeat(MAX_INTERESTS);
        assert_eq!(
            parse_interest_lines(&limit).unwrap(),
            vec!["tag".to_owned(); MAX_INTERESTS]
        );
        assert_eq!(
            parse_interest_lines(&format!("{limit}tag\n")),
            Err(DraftError::Interests)
        );
    }

    #[test]
    fn invalid_multiline_tag_never_returns_an_accepted_prefix() {
        for typed in [
            format!("valid\n{}", "x".repeat(MAX_FIELD_CHARS + 1)),
            "valid\nmu\u{202E}sic".into(),
            "valid\nx\ry".into(),
        ] {
            assert_eq!(parse_interest_lines(&typed), Err(DraftError::Interests));
        }
    }

    #[test]
    fn invalid_lists_never_return_a_partial_normalized_result() {
        for tags in [
            vec!["music".into(), "  ".into()],
            vec!["x".repeat(MAX_FIELD_CHARS + 1)],
            vec!["mu\u{202E}sic".into()],
            vec!["music".into(); MAX_INTERESTS + 1],
        ] {
            assert_eq!(normalize_interests(&tags), Err(DraftError::Interests));
        }
    }
}
