//! Local chat projection converted to desktop JSON without exposing internal request IDs.

use dyapp_client::store::messages::{ChatAttachment, ChatItem, ChatMessage, DeliveryState};
use dyapp_client::store::Conversation;

/// Conversation states exposed to the desktop without the request's internal message ID.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatState {
    None,
    RequestSent,
    RequestReceived,
    Open,
}

impl From<Conversation> for ChatState {
    fn from(state: Conversation) -> Self {
        match state {
            Conversation::None => Self::None,
            Conversation::RequestSent(_) => Self::RequestSent,
            Conversation::RequestReceived(_) => Self::RequestReceived,
            Conversation::Open => Self::Open,
        }
    }
}

/// Status of the current own content; node acceptance alone is not recipient delivery.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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
pub struct PhotoView {
    pub mime: String,
    /// The sender's claim, only to reserve layout space.
    pub width: u32,
    pub height: u32,
    pub size: u64,
    /// The bounded JPEG preview, empty in a list row and when the sender gave none.
    pub preview: Vec<u8>,
}

/// Current text, deletion marker or photo, in the Store's pair-wide order.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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
}

impl From<ChatMessage> for MessageView {
    fn from(message: ChatMessage) -> Self {
        Self {
            group_id: message.key.group,
            author: message.key.author,
            id: message.key.id,
            text: message
                .current
                .text
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
            edited: message.current.edited,
            deleted: message.current.deleted,
            unread: message.unread,
            forwarded: message.forwarded,
            delivery: message.delivery.map(Into::into),
            read: false,
            sent_at_ms: message.original_hlc.map(|hlc| hlc.wall_ms()),
            photo: None,
        }
    }
}

impl From<ChatAttachment> for MessageView {
    fn from(photo: ChatAttachment) -> Self {
        let sent_at_ms = Some(photo.sent_at_ms());
        let attachment = photo.message.attachment;
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
            photo: Some(PhotoView {
                mime: attachment.mime,
                width: attachment.width,
                height: attachment.height,
                size: attachment.size,
                preview: attachment.preview,
            }),
        }
    }
}

impl From<ChatItem> for MessageView {
    fn from(item: ChatItem) -> Self {
        match item {
            ChatItem::Text(message) => message.into(),
            ChatItem::Attachment(photo) => photo.into(),
        }
    }
}

/// A list row's newest entry. A row only needs to say it is a photo, and the list would otherwise
/// carry a preview per chat on every refresh.
pub(super) fn last_item(
    text: Option<ChatMessage>,
    photo: Option<ChatAttachment>,
) -> Option<MessageView> {
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
pub struct ChatRowView {
    pub peer_id: String,
    pub state: ChatState,
    pub last_message: Option<MessageView>,
    pub unread_count: usize,
    pub active_recently: bool,
}

/// One peer's current conversation, including an empty chat after Clear chat.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryView {
    pub state: ChatState,
    pub messages: Vec<MessageView>,
    pub active_recently: bool,
}

#[cfg(test)]
mod tests {
    use super::{last_item, DeliveryView, MessageView};
    use dyapp_client::attachment::Attachment;
    use dyapp_client::chat::AttachmentMessage;
    use dyapp_client::store::messages::{ChatAttachment, MessageKey};

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
        assert_eq!(fields, ["height", "mime", "preview", "size", "width"]);
        assert_eq!(json["photo"]["preview"], serde_json::json!([7, 8]));
        for secret in ["201", "202", "203", "204"] {
            assert!(
                !json.to_string().contains(secret),
                "{secret} reached the page"
            );
        }
    }

    #[test]
    fn a_list_row_names_the_photo_without_its_preview() {
        let row = last_item(None, Some(photo())).unwrap().photo.unwrap();
        assert!(row.preview.is_empty());
        assert_eq!(row.mime, "image/jpeg");
        assert!(last_item(None, None).is_none());
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
