//! The public native export rejects unsafe source/target selectors before network or staging.
use super::favorites::Fixture;
use super::*;
use crate::{ChatForwardError, ChatForwardSource};

fn source() -> ChatForwardSource {
    ChatForwardSource {
        peer_id: "ab".repeat(32),
        group_id: vec![1],
        author: "ab".repeat(32),
        message_id: vec![2; 16],
    }
}

#[test]
fn forward_export_uses_typed_preflight_and_never_stages_on_refusal() {
    let f = Fixture::new();
    let target = "cd".repeat(32);
    assert!(matches!(
        f.chats.forward_text(source(), target.clone()),
        Err(ChatForwardError::UnknownPeer)
    ));
    assert!(matches!(
        f.chats.forward_text(source(), source().peer_id),
        Err(ChatForwardError::InvalidTarget)
    ));
    assert!(matches!(
        f.chats.forward_text(source(), f.key.peer_id()),
        Err(ChatForwardError::InvalidTarget)
    ));
    let mut own = source();
    own.peer_id = f.key.peer_id();
    assert!(matches!(
        f.chats.forward_text(own, target.clone()),
        Err(ChatForwardError::OwnPeer)
    ));
    for blocked in [source().peer_id, target.clone()] {
        f.chats
            .session
            .lock()
            .unwrap()
            .store()
            .set_blocked(&blocked, true)
            .unwrap();
        assert!(matches!(
            f.chats.forward_text(source(), target.clone()),
            Err(ChatForwardError::Blocked)
        ));
        f.chats
            .session
            .lock()
            .unwrap()
            .store()
            .set_blocked(&blocked, false)
            .unwrap();
    }
    let session = f.chats.session.lock().unwrap();
    assert!(session.store().due_outbound(0).unwrap().is_empty());
    assert_eq!(
        session.store().conversation(&target).unwrap(),
        Conversation::None
    );
}
