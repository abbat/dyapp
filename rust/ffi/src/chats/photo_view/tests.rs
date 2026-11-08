use super::*;
use dyapp_client::attachment;
use dyapp_client::chat::AttachmentMessage;
use dyapp_client::store::messages::{DeliveryState, MessageKey};

#[test]
fn metadata_preserves_message_identity_and_clamps_untrusted_layout() {
    let (mut attachment, _) = attachment::seal(b"image").unwrap();
    attachment.width = u32::MAX;
    attachment.height = 1;
    attachment.mime = "image/jpeg".into();
    attachment.preview = vec![1, 2, 3];
    let stored = ChatAttachment {
        key: MessageKey {
            group: vec![4],
            author: "sender".into(),
            id: [5; 16],
        },
        message: AttachmentMessage {
            id: [5; 16],
            hlc: 123_456,
            attachment,
        },
        unread: true,
        forwarded: true,
        delivery: Some(DeliveryState::NodeAccepted),
    };
    let wall = stored.sent_at_ms();
    let projected = ChatPhoto::from(stored);
    assert_eq!(projected.group_id, vec![4]);
    assert_eq!(projected.author, "sender");
    assert_eq!(projected.id, vec![5; 16]);
    assert_eq!(projected.hlc, 123_456);
    assert_eq!(projected.sent_at_ms, wall);
    assert_eq!(projected.size, 5);
    assert_eq!(projected.mime, "image/jpeg");
    assert_eq!(projected.preview, vec![1, 2, 3]);
    assert_eq!((projected.width, projected.height), (20, 1));
    assert!(projected.unread && projected.forwarded);
    assert_eq!(projected.delivery, Some(ChatDelivery::NodeAccepted));
}
