use super::{for_row, text::bounded, ChatPreviewBody};
use crate::store::messages::{
    ChatAttachment, ChatMessage, ChatSticker, CurrentMessage, MessageKey,
};
use crate::store::{ChatRow, Conversation};
use dyapp_messaging::Timestamp;
use unicode_segmentation::UnicodeSegmentation;

const PEER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn key() -> MessageKey {
    MessageKey {
        group: vec![1],
        author: PEER.into(),
        id: [1; 16],
    }
}

fn row(content: &str) -> ChatRow {
    ChatRow {
        peer_id: PEER.into(),
        state: Conversation::Open,
        last_message: Some(ChatMessage {
            key: key(),
            current: CurrentMessage {
                text: Some(content.as_bytes().to_vec()),
                edited: false,
                deleted: false,
            },
            original_hlc: Some(Timestamp::new(12_345, 7)),
            unread: false,
            forwarded: false,
            delivery: None,
        }),
        last_attachment: None,
        last_sticker: None,
        unread_count: 0,
    }
}

#[test]
fn exact_boundary_is_complete_and_long_text_has_a_visible_marker() {
    assert_eq!(bounded(&"a".repeat(80)), "a".repeat(80));
    assert_eq!(bounded(&"a".repeat(81)), format!("{}…", "a".repeat(79)));
    assert_eq!(bounded(""), "");
}

#[test]
fn whitespace_and_controls_form_one_readable_line_without_mutating_history() {
    let original = "  hello\r\n\tworld\0\u{1b} next  ";
    let actual = row(original);
    assert_eq!(
        for_row(&actual).body,
        ChatPreviewBody::Text {
            text: "hello world next".into()
        }
    );
    assert_eq!(
        actual
            .last_message
            .as_ref()
            .unwrap()
            .current
            .text
            .as_deref(),
        Some(original.as_bytes()),
    );
}

#[test]
fn emoji_and_combining_sequences_are_never_split_at_the_grapheme_limit() {
    let emoji = "👩🏽‍🚀";
    let accent = "e\u{301}";
    let exact = format!("{}{emoji}{accent}", "a".repeat(78));
    assert_eq!(bounded(&exact), exact);
    let long = format!("{}{emoji}{accent}x", "a".repeat(78));
    assert_eq!(bounded(&long), format!("{}{emoji}…", "a".repeat(78)));
}

#[test]
fn byte_bound_also_handles_long_emoji_and_pathological_single_graphemes() {
    let family = "👨‍👩‍👧‍👦";
    let result = bounded(&family.repeat(80));
    assert!(result.ends_with('…'));
    assert!(result.len() <= 512);
    assert!(result.graphemes(true).count() <= 80);
    assert!(result
        .trim_end_matches('…')
        .graphemes(true)
        .all(|part| part == family));
    let oversized = format!("a{}", "\u{301}".repeat(1_000));
    assert_eq!(bounded(&oversized), "…");
}

#[test]
fn read_and_forwarded_text_remain_visible_with_the_original_timestamp() {
    let mut actual = row(&"Ж".repeat(100));
    actual.last_message.as_mut().unwrap().forwarded = true;
    let preview = for_row(&actual);
    assert_eq!(
        preview.body,
        ChatPreviewBody::Text {
            text: format!("{}…", "Ж".repeat(79))
        }
    );
    assert!(preview.forwarded);
    assert_eq!(preview.sent_at_ms, Some(12_345));
    assert_eq!(
        actual.last_message.unwrap().current.text.unwrap(),
        "Ж".repeat(100).as_bytes()
    );
}

#[test]
fn deleted_and_empty_rows_do_not_borrow_retained_or_previous_content() {
    let mut actual = row("must not appear");
    actual.last_message.as_mut().unwrap().current.deleted = true;
    actual.last_message.as_mut().unwrap().forwarded = true;
    assert_eq!(for_row(&actual).body, ChatPreviewBody::Deleted);
    assert!(!for_row(&actual).forwarded);
    actual.last_message = None;
    let preview = for_row(&actual);
    assert_eq!(preview.body, ChatPreviewBody::Empty);
    assert_eq!(preview.sent_at_ms, None);
}

#[test]
fn only_a_complete_valid_profile_link_is_a_profile_preview() {
    let link = format!("dyapp-profile:1:{PEER}");
    assert_eq!(
        for_row(&row(&link)).body,
        ChatPreviewBody::Profile {
            peer_id: PEER.into()
        }
    );
    assert!(matches!(
        for_row(&row(&format!("see {link}"))).body,
        ChatPreviewBody::Text { .. }
    ));
}

#[test]
fn newest_media_dominates_retained_text_and_preserves_its_own_time() {
    let mut actual = row("older text");
    let (attachment, _) = crate::attachment::seal(b"fixture").unwrap();
    actual.last_attachment = Some(ChatAttachment {
        key: key(),
        message: crate::chat::AttachmentMessage {
            id: [1; 16],
            hlc: Timestamp::new(20_000, 1).into(),
            attachment,
        },
        unread: false,
        forwarded: true,
        delivery: None,
    });
    let photo = for_row(&actual);
    assert_eq!(photo.body, ChatPreviewBody::Photo);
    assert!(photo.forwarded);
    assert_eq!(photo.sent_at_ms, Some(20_000));
    actual.last_sticker = Some(ChatSticker {
        key: key(),
        sticker: crate::chat::sticker::Sticker {
            id: [2; 16],
            hlc: Timestamp::new(30_000, 2).into(),
            pack_id: [3; 32],
            sha256: [4; 32],
            emoji: "👋".into(),
            pack_name: "Hello".into(),
            attachment: None,
        },
        unread: false,
        delivery: None,
    });
    let sticker = for_row(&actual);
    assert_eq!(
        sticker.body,
        ChatPreviewBody::Sticker {
            emoji: "👋".into(),
            pack_name: "Hello".into()
        }
    );
    assert_eq!(sticker.sent_at_ms, Some(30_000));
    assert!(!sticker.forwarded);
}
