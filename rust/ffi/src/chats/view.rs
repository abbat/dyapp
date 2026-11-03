//! The local chat projection as records for the app, without internal request IDs.

use super::ChatState;
use dyapp_client::store;
use dyapp_client::store::messages::{self, DeliveryState};

/// The like state with one peer as the screens show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatLikes {
    /// This profile likes the peer: the screen shows Liked.
    pub ours: bool,
    /// Both like each other.
    pub matched: bool,
    /// This profile passes on the peer: search and Liked you hide them; the peer is not told.
    pub passed: bool,
}

/// Status of the current own content; node acceptance alone is not recipient delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatDelivery {
    /// Staged here, no node took it yet.
    Pending,
    /// A node took it.
    NodeAccepted,
    /// A device of the peer received it.
    DeviceReceived,
}

impl From<DeliveryState> for ChatDelivery {
    fn from(value: DeliveryState) -> Self {
        match value {
            DeliveryState::Pending => Self::Pending,
            DeliveryState::NodeAccepted => Self::NodeAccepted,
            DeliveryState::DeviceReceived => Self::DeviceReceived,
        }
    }
}

/// Current text or deletion marker, in the Store's pair-wide order.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatMessage {
    /// The MLS group it was sent in.
    pub group_id: Vec<u8>,
    /// The author's peer ID.
    pub author: String,
    /// The 16-byte logical ID that edits and deletions refer to.
    pub id: Vec<u8>,
    /// The current text, absent once deleted.
    pub text: Option<String>,
    /// The author edited it.
    pub edited: bool,
    /// The author deleted it.
    pub deleted: bool,
    /// Received and not yet read here.
    pub unread: bool,
    /// The author forwarded it from another chat.
    pub forwarded: bool,
    /// Delivery of an own message; absent on a received one.
    pub delivery: Option<ChatDelivery>,
    /// The peer's read receipt covers this live own text; set only in [`ChatHistory`].
    pub read: bool,
    /// Original wall time for timed text; legacy text has no timestamp.
    pub sent_at_ms: Option<u64>,
}

impl From<messages::ChatMessage> for ChatMessage {
    fn from(message: messages::ChatMessage) -> Self {
        Self {
            group_id: message.key.group,
            author: message.key.author,
            id: message.key.id.to_vec(),
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
        }
    }
}

/// One Chats list row from this identity's local Store.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatRow {
    /// The peer's ID.
    pub peer_id: String,
    /// Where the pair's chat stands.
    pub state: ChatState,
    /// The newest message, absent in a cleared or still empty chat.
    pub last_message: Option<ChatMessage>,
    /// Messages not yet read here.
    pub unread_count: u64,
    /// Whether this profile mutes the peer: no notifications, the chat itself unchanged.
    pub muted: bool,
}

impl ChatRow {
    pub(crate) fn new(row: store::ChatRow, muted: bool) -> Self {
        Self {
            peer_id: row.peer_id,
            state: row.state.into(),
            last_message: row.last_message.map(Into::into),
            unread_count: row.unread_count as u64,
            muted,
        }
    }
}

/// One peer's current conversation, including an empty chat after Clear chat.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatHistory {
    /// Where the pair's chat stands.
    pub state: ChatState,
    /// The current messages in the Store's pair-wide order.
    pub messages: Vec<ChatMessage>,
    /// Whether this profile mutes the peer.
    pub muted: bool,
}
