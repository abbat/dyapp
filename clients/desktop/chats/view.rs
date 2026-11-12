//! Local chat projection converted to desktop JSON without exposing internal request IDs.

use dyapp_client::attachment::{hours_left, layout_size};
use dyapp_client::chat_composer as composer;
use dyapp_client::store::messages::{
    ChatAttachment, ChatItem, ChatMessage, ChatSticker, DeliveryState,
};
use dyapp_client::store::originals;
use dyapp_client::store::Conversation;
use dyapp_identity::profile_link::parse_profile_link;
use std::path::Path;

/// Network revocation is only confirmed by the node reached in this attempt.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub enum Revocation {
    NotNeeded,
    Pending,
    Confirmed,
}

/// Local block always remains effective when network revocation is pending.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct BlockResult {
    pub revocation: Revocation,
    pub detail: Option<String>,
}

impl BlockResult {
    pub(super) fn pending(detail: &(impl ToString + ?Sized)) -> Self {
        Self {
            revocation: Revocation::Pending,
            detail: Some(detail.to_string()),
        }
    }
}

/// Conversation states exposed to the desktop without the request's internal message ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ChatState {
    None,
    RequestSent,
    RequestReceived,
    Open,
    Closed,
}

impl From<Conversation> for ChatState {
    fn from(state: Conversation) -> Self {
        match state {
            Conversation::None => Self::None,
            Conversation::RequestSent(_) => Self::RequestSent,
            Conversation::RequestReceived(_) => Self::RequestReceived,
            Conversation::Open => Self::Open,
            Conversation::Closed => Self::Closed,
        }
    }
}

impl From<ChatState> for composer::State {
    fn from(state: ChatState) -> Self {
        match state {
            ChatState::None => Self::None,
            ChatState::RequestSent => Self::RequestSent,
            ChatState::RequestReceived => Self::RequestReceived,
            ChatState::Open => Self::Open,
            ChatState::Closed => Self::Closed,
        }
    }
}

/// Shared core composer controls; sending still checks authorization and limits itself.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComposerView {
    too_long: bool,
    blank: bool,
    sendable: bool,
}

/// Assess composer text for a chat phase; no phase means the chat is still loading.
pub(crate) fn composer(state: Option<ChatState>, text: &str) -> ComposerView {
    let policy = composer::assess(state.map(composer::State::from), text);
    ComposerView {
        too_long: policy.too_long,
        blank: policy.blank,
        sendable: policy.sendable,
    }
}

/// Status of the current own content; node acceptance alone is not recipient delivery.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub enum DeliveryView {
    Pending,
    NodeAccepted,
    DeviceReceived,
}

impl From<DeliveryState> for DeliveryView {
    fn from(value: DeliveryState) -> Self {
        match value {
            DeliveryState::Pending => Self::Pending,
            DeliveryState::NodeAccepted => Self::NodeAccepted,
            DeliveryState::DeviceReceived => Self::DeviceReceived,
        }
    }
}

/// What a bubble shows of a photo. Built field by field, so the key and the release secret that
/// open the original never reach the page.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct PhotoView {
    pub mime: String,
    /// The sender's claimed shape with its aspect ratio clamped, only to reserve layout space.
    pub width: u32,
    pub height: u32,
    pub size: u64,
    /// Whole hours until nodes drop the original, set in history once under a day remains.
    pub hours_left: Option<u32>,
    /// This device keeps the original, so it opens without a node; set in history.
    pub kept: bool,
    /// Finds the kept original. Stays in the backend.
    #[serde(skip)]
    pub blob: [u8; 32],
    /// The bounded JPEG preview, empty in a list row and when the sender gave none.
    pub preview: Vec<u8>,
}

/// Safe sticker fallback metadata; no fetch descriptor or compressed bytes reach the page.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct StickerView {
    pub pack_id: [u8; 32],
    pub sha256: [u8; 32],
    pub emoji: String,
    pub pack_name: String,
}

/// Current text, deletion marker, photo or sticker, in the Store's pair-wide order.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent message flags retain the desktop IPC schema"
)]
pub struct MessageView {
    pub group_id: Vec<u8>,
    pub author: String,
    pub id: [u8; 16],
    pub text: Option<String>,
    pub edited: bool,
    pub deleted: bool,
    pub unread: bool,
    pub forwarded: bool,
    pub delivery: Option<DeliveryView>,
    /// The peer's read receipt covers this own text; set for history only.
    pub read: bool,
    /// Original wall time for timed text; legacy tag-1 text has no timestamp.
    pub sent_at_ms: Option<u64>,
    /// Set for a photo, which has no `text` and is neither edited nor deleted.
    pub photo: Option<PhotoView>,
    pub sticker: Option<StickerView>,
    /// The Peer ID of a text that is exactly a shared profile link, shown as a profile card.
    pub shared_peer: Option<String>,
}

impl From<ChatMessage> for MessageView {
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "Timestamp belongs to a transitive dependency, so its method cannot be imported"
    )]
    fn from(message: ChatMessage) -> Self {
        let text = message
            .current
            .text
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        Self {
            group_id: message.key.group,
            author: message.key.author,
            id: message.key.id,
            shared_peer: text
                .as_deref()
                .and_then(parse_profile_link)
                .map(str::to_owned),
            text,
            edited: message.current.edited,
            deleted: message.current.deleted,
            unread: message.unread,
            forwarded: message.forwarded,
            delivery: message.delivery.map(Into::into),
            read: false,
            sent_at_ms: message.original_hlc.map(|hlc| hlc.wall_ms()),
            photo: None,
            sticker: None,
        }
    }
}

impl From<ChatAttachment> for MessageView {
    fn from(photo: ChatAttachment) -> Self {
        let sent_at_ms = Some(photo.sent_at_ms());
        let attachment = photo.message.attachment;
        let (width, height) = layout_size(attachment.width, attachment.height);
        Self {
            group_id: photo.key.group,
            author: photo.key.author,
            id: photo.key.id,
            text: None,
            edited: false,
            deleted: false,
            unread: photo.unread,
            forwarded: photo.forwarded,
            delivery: photo.delivery.map(Into::into),
            read: false,
            sent_at_ms,
            sticker: None,
            photo: Some(PhotoView {
                mime: attachment.mime,
                width,
                height,
                size: attachment.size,
                hours_left: None,
                kept: false,
                blob: attachment.blob_sha256,
                preview: attachment.preview,
            }),
            shared_peer: None,
        }
    }
}

impl From<ChatSticker> for MessageView {
    fn from(stored: ChatSticker) -> Self {
        let sent_at_ms = Some(stored.sent_at_ms());
        Self {
            group_id: stored.key.group,
            author: stored.key.author,
            id: stored.key.id,
            text: None,
            edited: false,
            deleted: false,
            unread: stored.unread,
            forwarded: false,
            delivery: stored.delivery.map(Into::into),
            read: false,
            sent_at_ms,
            photo: None,
            shared_peer: None,
            sticker: Some(StickerView {
                pack_id: stored.sticker.pack_id,
                sha256: stored.sticker.sha256,
                emoji: stored.sticker.emoji,
                pack_name: stored.sticker.pack_name,
            }),
        }
    }
}

impl From<ChatItem> for MessageView {
    fn from(item: ChatItem) -> Self {
        match item {
            ChatItem::Text(message) => message.into(),
            ChatItem::Attachment(photo) => photo.into(),
            ChatItem::Sticker(sticker) => sticker.into(),
        }
    }
}

const HOUR_MS: i64 = 3_600_000;

fn photo_hours_left(sent_ms: u64, now_ms: u64) -> Option<u32> {
    let times = i64::try_from(sent_ms)
        .ok()
        .and_then(|sent| sent.checked_add(HOUR_MS - 1))
        .zip(i64::try_from(now_ms).ok());
    times.map_or(Some(0), |(sent, now)| hours_left(sent, now))
}

/// Label the photos of a history with the time left of their originals and those kept here.
pub(super) fn mark_photos(views: &mut [MessageView], now_ms: u64, dir: &Path) {
    for view in views {
        if let (Some(photo), Some(sent)) = (&mut view.photo, view.sent_at_ms) {
            photo.kept = originals::is_kept(dir, &photo.blob);
            // Round up valid times; unsupported clocks must never extend the label's lifetime.
            photo.hours_left = photo_hours_left(sent, now_ms);
        }
    }
}

/// A list row's newest entry. A row only needs to say it is a photo, and the list would otherwise
/// carry a preview per chat on every refresh.
pub(super) fn last_item(
    text: Option<ChatMessage>,
    photo: Option<ChatAttachment>,
    sticker: Option<ChatSticker>,
) -> Option<MessageView> {
    if let Some(sticker) = sticker {
        return Some(sticker.into());
    }
    let Some(photo) = photo else {
        return text.map(Into::into);
    };
    let mut view = MessageView::from(photo);
    if let Some(photo) = &mut view.photo {
        photo.preview = Vec::new();
    }
    Some(view)
}

/// One Chats list row from this identity's local Store.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct ChatRowView {
    pub peer_id: String,
    pub state: ChatState,
    pub last_message: Option<MessageView>,
    pub unread_count: usize,
    pub active_recently: bool,
    /// Mail from this peer adds no notice; see `notify::waiting`.
    pub muted: bool,
}

/// One peer's current conversation, including an empty chat after Clear chat.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct ChatHistoryView {
    pub state: ChatState,
    pub messages: Vec<MessageView>,
    pub active_recently: bool,
    pub muted: bool,
}

#[cfg(test)]
mod tests {
    use super::{last_item, mark_photos, photo_hours_left, DeliveryView, MessageView, HOUR_MS};
    use dyapp_client::attachment::{Attachment, RETENTION_HOURS};
    use dyapp_client::chat::AttachmentMessage;
    use dyapp_client::store::messages::{ChatAttachment, MessageKey};
    use std::path::Path;

    #[test]
    fn composer_reads_ui_states_and_follows_core_policy() {
        let open: super::ChatState = serde_json::from_str("\"open\"").unwrap();
        let waiting: super::ChatState = serde_json::from_str("\"requestSent\"").unwrap();
        let view = |state, text| serde_json::to_value(super::composer(state, text)).unwrap();
        let expect = |too_long, blank, sendable| serde_json::json!({ "tooLong": too_long, "blank": blank, "sendable": sendable });
        assert_eq!(view(Some(open), "hi"), expect(false, false, true));
        assert_eq!(view(Some(open), " \n"), expect(false, true, false));
        assert_eq!(view(Some(waiting), "hi"), expect(false, false, false));
        assert_eq!(view(None, "hi"), expect(false, false, false));
        let long = "a".repeat(dyapp_client::chat::MAX_TEXT_BYTES + 1);
        assert_eq!(view(Some(open), &long), expect(true, false, false));
    }

    #[test]
    fn photo_expiry_labels_reject_unrepresentable_times() {
        assert_eq!(photo_hours_left(0, u64::MAX), Some(0));
        assert_eq!(photo_hours_left(u64::MAX, 0), Some(0));
        let signed_max = u64::try_from(i64::MAX).unwrap();
        assert_eq!(photo_hours_left(signed_max, 0), Some(0));
        let hour = u64::try_from(HOUR_MS).unwrap();
        let retention = u64::from(RETENTION_HOURS) * hour;
        assert_eq!(photo_hours_left(0, retention), Some(0));
        assert_eq!(photo_hours_left(0, retention - 1), Some(1));
        assert_eq!(photo_hours_left(0, retention - hour), Some(1));
        assert_eq!(photo_hours_left(0, retention - hour - 1), Some(2));
    }

    #[test]
    fn message_view_keeps_legacy_time_absent_and_uses_safe_wall_time() {
        use dyapp_client::store::messages::{ChatMessage, CurrentMessage};
        let message = |original_hlc, forwarded| ChatMessage {
            key: MessageKey {
                group: vec![1],
                author: "peer".into(),
                id: [2; 16],
            },
            current: CurrentMessage {
                text: Some(b"hello".to_vec()),
                edited: false,
                deleted: false,
            },
            original_hlc,
            unread: true,
            forwarded,
            delivery: None,
        };
        assert_eq!(MessageView::from(message(None, false)).sent_at_ms, None);
        let timed = MessageView::from(message(Some((1_700_000_000_000u64 << 16).into()), true));
        assert_eq!(timed.sent_at_ms, Some(1_700_000_000_000));
        assert_eq!(timed.text.as_deref(), Some("hello"));
        assert!(timed.forwarded);
        assert_eq!(serde_json::to_value(&timed).unwrap()["forwarded"], true);
        assert!(timed.unread);
        assert_eq!(timed.shared_peer, None);
        let peer = "ab".repeat(32);
        let mut shared = message(None, false);
        shared.current.text = Some(format!("dyapp-profile:1:{peer}").into_bytes());
        assert_eq!(
            MessageView::from(shared.clone()).shared_peer,
            Some(peer.clone())
        );
        // Only the whole body is a link.
        shared.current.text = Some(format!("see dyapp-profile:1:{peer}").into_bytes());
        assert_eq!(MessageView::from(shared).shared_peer, None);
    }

    #[test]
    fn sticker_view_exposes_only_fallback_metadata() {
        let media = photo();
        let stored = dyapp_client::store::messages::ChatSticker {
            key: media.key,
            sticker: dyapp_client::chat::sticker::Sticker {
                id: [2; 16],
                hlc: 5 << 16,
                pack_id: [1; 32],
                sha256: [2; 32],
                emoji: "😺".into(),
                pack_name: "Cats".into(),
                attachment: Some(media.message.attachment),
            },
            unread: true,
            delivery: None,
        };
        let view = last_item(None, None, Some(stored)).unwrap();
        assert_eq!(view.sent_at_ms, Some(5));
        assert!(view.text.is_none() && view.photo.is_none() && view.unread);
        let json = serde_json::to_value(view).unwrap();
        let mut fields: Vec<_> = json["sticker"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        fields.sort();
        assert_eq!(fields, ["emoji", "packId", "packName", "sha256"]);
        assert_eq!(json["sticker"]["packName"], "Cats");
    }

    fn photo() -> ChatAttachment {
        ChatAttachment {
            key: MessageKey {
                group: vec![1],
                author: "peer".into(),
                id: [2; 16],
            },
            message: AttachmentMessage {
                id: [2; 16],
                hlc: 5 << 16,
                attachment: Attachment {
                    key: [201; 32],
                    plaintext_sha256: [202; 32],
                    blob_sha256: [203; 32],
                    release_secret: [204; 32],
                    size: 12,
                    preview: vec![7, 8],
                    mime: "image/jpeg".into(),
                    width: 20,
                    height: 10,
                },
            },
            unread: true,
            forwarded: false,
            delivery: None,
        }
    }

    #[test]
    fn a_photo_reaches_the_page_without_its_secrets() {
        let view = MessageView::from(photo());
        assert_eq!(view.sent_at_ms, Some(5));
        assert!(view.unread && view.text.is_none());
        let json = serde_json::to_value(&view).unwrap();
        let photo_json = json["photo"].as_object().unwrap();
        let mut fields: Vec<_> = photo_json.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "height",
                "hoursLeft",
                "kept",
                "mime",
                "preview",
                "size",
                "width"
            ]
        );
        assert_eq!(json["photo"]["preview"], serde_json::json!([7, 8]));
        for secret in ["201", "202", "203", "204"] {
            assert!(
                !json.to_string().contains(secret),
                "{secret} reached the page"
            );
        }
    }

    #[test]
    fn only_the_last_day_of_a_photo_is_labelled() {
        let hour = 3_600_000;
        let mut views = [MessageView::from(photo())];
        for (now, left) in [
            (5, None),
            (5 + 163 * hour, Some(5)),
            (5 + 168 * hour - 1, Some(1)),
            (5 + 168 * hour, Some(0)),
        ] {
            mark_photos(&mut views, now, Path::new("/nonexistent"));
            assert_eq!(views[0].photo.as_ref().unwrap().hours_left, left);
        }
    }

    #[test]
    fn a_list_row_names_the_photo_without_its_preview() {
        let row = last_item(None, Some(photo()), None).unwrap().photo.unwrap();
        assert_eq!(row.preview, Vec::<u8>::new());
        assert_eq!(row.mime, "image/jpeg");
        assert!(last_item(None, None, None).is_none());
    }

    #[test]
    fn delivery_wire_names_distinguish_node_and_device() {
        assert_eq!(
            serde_json::to_string(&DeliveryView::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&DeliveryView::NodeAccepted).unwrap(),
            "\"nodeAccepted\""
        );
        assert_eq!(
            serde_json::to_string(&DeliveryView::DeviceReceived).unwrap(),
            "\"deviceReceived\""
        );
    }
}
