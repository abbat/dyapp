//! Native limits are checked against actual parsing, not another copied policy constant.
use super::{parse_profile_interests, profile_interest_limits};

#[test]
fn native_multiline_interests_keep_canonical_punctuation_order_and_clear_intent() {
    let parsed =
        parse_profile_interests(" Music, Art; Books\r\n\nE\u{301}COLE\nmusic, art; books".into())
            .unwrap();
    assert_eq!(parsed, ["music, art; books", "école", "music, art; books"]);
    assert_eq!(parse_profile_interests(" \n\t".into()), Some(Vec::new()));
    assert_eq!(parse_profile_interests("valid\nmu\u{202E}sic".into()), None);
}

#[test]
fn advertised_native_line_limit_matches_actual_parser_admission() {
    let limit = usize::try_from(profile_interest_limits().max_tags).unwrap();
    let accepted = "tag\n\n".repeat(limit);
    assert_eq!(
        parse_profile_interests(accepted.clone()).unwrap().len(),
        limit
    );
    assert_eq!(parse_profile_interests(format!("{accepted}tag\n")), None);
}

#[test]
fn advertised_native_canonical_character_limit_matches_actual_parser_admission() {
    let limit = usize::try_from(profile_interest_limits().max_tag_chars).unwrap();
    let accepted = "x".repeat(limit);
    assert_eq!(
        parse_profile_interests(accepted.clone()).unwrap(),
        [accepted.as_str()]
    );
    assert_eq!(parse_profile_interests(format!("{accepted}x")), None);
}
