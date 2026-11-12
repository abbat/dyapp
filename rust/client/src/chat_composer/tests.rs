use super::*;

#[test]
fn all_stored_phases_and_loading_have_explicit_composer_policy() {
    for (conversation, writable) in [
        (Conversation::None, true),
        (Conversation::RequestSent([1; 16]), false),
        (Conversation::RequestReceived([2; 16]), true),
        (Conversation::Open, true),
        (Conversation::Closed, false),
    ] {
        let policy = assess(Some(State::from(conversation)), "text");
        assert_eq!(policy.writable, writable);
        assert_eq!(policy.sendable, writable);
        assert!(!policy.too_long);
    }
    assert!(!assess(None, "text").writable);
    assert!(!assess(None, "text").sendable);
}

#[test]
fn text_limit_is_inclusive_utf8_bytes_for_every_phase() {
    for (unit, count) in [("a", 65_536), ("Ж", 32_768), ("😀", 16_384)] {
        let boundary = unit.repeat(count);
        assert_eq!(boundary.len(), 65_536);
        assert!(assess(Some(State::Open), &boundary).sendable);
        assert!(!assess(Some(State::Open), &boundary).too_long);
        let over = boundary + unit;
        assert!(!assess(Some(State::Open), &over).sendable);
        assert!(assess(None, &over).too_long);
    }
}

#[test]
fn blank_text_matches_the_linux_javascript_composer() {
    for blank in ["", " \t\r\n", "\u{a0}", "\u{2007}\u{202f}", "\u{feff}"] {
        assert!(!assess(Some(State::Open), blank).sendable);
        assert!(assess(Some(State::Open), blank).writable);
        assert!(assess(None, blank).blank);
    }
    for nonblank in ["text", "  text\n", "\u{85}", "\u{1c}"] {
        assert!(assess(Some(State::Open), nonblank).sendable);
        assert!(!assess(None, nonblank).blank);
    }
}
