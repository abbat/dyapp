//! Local chat projection converted to desktop JSON without exposing internal request IDs.

use dyapp_client::store::messages::{ChatMessage, DeliveryState};
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

/// Current text or deletion marker, in the Store's pair-wide order.
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
    /// Original wall time for timed text; legacy tag-1 text has no timestamp.
    pub sent_at_ms: Option<u64>,
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
            sent_at_ms: message.original_hlc.map(|hlc| hlc.wall_ms()),
        }
    }
}

/// One Chats list row from this identity's local Store.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRowView {
    pub peer_id: String,
    pub state: ChatState,
    pub last_message: Option<MessageView>,
    pub unread_count: usize,
}

/// One peer's current conversation, including an empty chat after Clear chat.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryView {
    pub state: ChatState,
    pub messages: Vec<MessageView>,
}

#[cfg(test)]
mod tests {
    use super::DeliveryView;

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
