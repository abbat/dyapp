use super::*;
use crate::store::messages::{ChatAttachment, ChatMessage, CurrentMessage, MessageKey};

pub(super) fn row(peer: &str, state: Conversation, text: &str, unread_count: usize) -> ChatRow {
    ChatRow {
        peer_id: peer.into(),
        state,
        last_message: Some(ChatMessage {
            key: MessageKey {
                group: vec![1],
                author: peer.into(),
                id: [1; 16],
            },
            current: CurrentMessage {
                text: Some(text.as_bytes().to_vec()),
                edited: false,
                deleted: false,
            },
            original_hlc: None,
            unread: true,
            forwarded: false,
            delivery: None,
        }),
        last_attachment: None,
        last_sticker: None,
        unread_count,
    }
}

#[test]
fn mode_controls_sender_and_plain_content_without_platform_markup() {
    let after = [row("peer", Conversation::Open, "<b>hi</b> & bye\nnext", 1)];
    assert_eq!(content(NoticeContent::Fixed, &[], &after), None);
    let name = content(NoticeContent::Name, &[], &after).unwrap();
    assert_eq!(name.peer_id, "peer");
    assert_eq!(name.preview, Preview::Empty);
    let text = content(NoticeContent::Text, &[], &after).unwrap();
    assert_eq!(text.peer_id, "peer");
    assert_eq!(text.preview.plain_text("Sticker"), "<b>hi</b> & bye next");
}

#[test]
fn no_increase_or_newest_request_never_borrows_another_open_chats_text() {
    let before = [row("peer", Conversation::Open, "seen", 1)];
    assert_eq!(content(NoticeContent::Text, &before, &before), None);
    let decreased = [row("peer", Conversation::Open, "seen", 0)];
    assert_eq!(content(NoticeContent::Text, &before, &decreased), None);
    for state in [
        Conversation::RequestReceived([1; 16]),
        Conversation::RequestSent([1; 16]),
        Conversation::Closed,
    ] {
        let after = [
            row("new", state, "stranger", 1),
            row("open", Conversation::Open, "private", 1),
        ];
        assert_eq!(content(NoticeContent::Text, &before, &after), None);
    }
}

#[test]
fn plain_body_bounds_unicode_scalars_and_replaces_controls_before_escaping() {
    let text = format!("a\nb\u{85}{}", "😀".repeat(300));
    let shown = Preview::Text(text).plain_text("Sticker");
    assert!(shown.starts_with("a b "));
    assert_eq!(shown.chars().count(), 200);
    assert!(!shown.contains('\u{fffd}'));
    assert_eq!(
        Preview::Sticker("😺".into()).plain_text("Стикер"),
        "😺 Стикер"
    );
    assert_eq!(Preview::Empty.plain_text("Sticker"), "");
}

#[test]
fn current_sticker_marker_dominates_retained_older_text_even_when_already_read() {
    let f = crate::sticker_media::tests::Fixture::new(true, false);
    let mut actual = f.store.chat_rows().unwrap().remove(0);
    assert!(actual.last_sticker.is_some() && actual.last_message.is_some());
    actual.last_message.as_mut().unwrap().current.text = Some(b"older private words".to_vec());
    let selected = content(NoticeContent::Text, &[], std::slice::from_ref(&actual)).unwrap();
    assert_eq!(selected.preview.plain_text("Sticker"), "😺 Sticker");
    actual.last_sticker.as_mut().unwrap().unread = false;
    assert_eq!(body(&actual), Preview::Empty);
    actual.last_sticker.as_mut().unwrap().unread = true;
    assert_eq!(
        content(NoticeContent::Name, &[], std::slice::from_ref(&actual))
            .unwrap()
            .preview,
        Preview::Empty
    );
    actual.state = Conversation::Closed;
    assert_eq!(
        content(NoticeContent::Text, &[], std::slice::from_ref(&actual)),
        None
    );
}

#[test]
fn photo_marker_and_invalid_or_read_text_cannot_expose_retained_words() {
    let mut actual = row("peer", Conversation::Open, "older private words", 2);
    let (attachment, _) = crate::attachment::seal(b"photo fixture").unwrap();
    actual.last_attachment = Some(ChatAttachment {
        key: MessageKey {
            group: vec![1],
            author: "peer".into(),
            id: [2; 16],
        },
        message: crate::chat::AttachmentMessage {
            id: [2; 16],
            hlc: 2,
            attachment,
        },
        unread: true,
        forwarded: false,
        delivery: None,
    });
    assert_eq!(body(&actual), Preview::Empty);
    actual.last_attachment.as_mut().unwrap().unread = false;
    assert_eq!(body(&actual), Preview::Empty);
    actual.last_attachment = None;
    actual.last_message.as_mut().unwrap().unread = false;
    assert_eq!(body(&actual), Preview::Empty);
    actual.last_message.as_mut().unwrap().unread = true;
    actual.last_message.as_mut().unwrap().current.text = Some(vec![255]);
    assert_eq!(body(&actual), Preview::Empty);
}
