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
