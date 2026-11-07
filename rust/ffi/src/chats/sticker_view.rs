//! Sticker metadata only; transport descriptors and pixels stay in the core.

use super::ChatDelivery;

/// A sticker fallback in merged history, without any fetch or release secret.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatSticker {
    /// MLS group and verified author scope the logical ID.
    pub group_id: Vec<u8>,
    /// Verified sender Peer ID.
    pub author: String,
    /// Logical message ID.
    pub id: Vec<u8>,
    /// Original ordering HLC.
    pub hlc: u64,
    /// Authenticated wall time.
    pub sent_at_ms: u64,
    /// Immutable pack ID.
    pub pack_id: Vec<u8>,
    /// Plaintext sticker hash.
    pub sha256: Vec<u8>,
    /// One clean grapheme.
    pub emoji: String,
    /// Clean pack name.
    pub pack_name: String,
    /// Local unread state.
    pub unread: bool,
    /// Own-message delivery status.
    pub delivery: Option<ChatDelivery>,
}

impl From<dyapp_client::store::messages::ChatSticker> for ChatSticker {
    fn from(stored: dyapp_client::store::messages::ChatSticker) -> Self {
        let sent_at_ms = stored.sent_at_ms();
        Self {
            group_id: stored.key.group,
            author: stored.key.author,
            id: stored.key.id.to_vec(),
            hlc: stored.sticker.hlc,
            sent_at_ms,
            pack_id: stored.sticker.pack_id.to_vec(),
            sha256: stored.sticker.sha256.to_vec(),
            emoji: stored.sticker.emoji,
            pack_name: stored.sticker.pack_name,
            unread: stored.unread,
            delivery: stored.delivery.map(Into::into),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn metadata_retains_identity_order_and_labels_without_descriptor() {
        use dyapp_client::store::messages::{ChatSticker as Stored, MessageKey};
        let sticker = dyapp_client::chat::sticker::Sticker {
            id: [1; 16],
            hlc: 7 << 16,
            pack_id: [2; 32],
            sha256: [3; 32],
            emoji: "😺".into(),
            pack_name: "Cats".into(),
            attachment: None,
        };
        let projected = super::ChatSticker::from(Stored {
            key: MessageKey {
                group: vec![9],
                author: "peer".into(),
                id: sticker.id,
            },
            sticker,
            unread: true,
            delivery: None,
        });
        assert_eq!(
            (projected.group_id, projected.author, projected.id),
            (vec![9], "peer".into(), vec![1; 16])
        );
        assert_eq!((projected.hlc, projected.sent_at_ms), (7 << 16, 7));
        assert_eq!(
            (projected.pack_id, projected.sha256),
            (vec![2; 32], vec![3; 32])
        );
        assert_eq!(
            (projected.emoji.as_str(), projected.pack_name.as_str()),
            ("😺", "Cats")
        );
        assert!(projected.unread && projected.delivery.is_none());
    }
}
