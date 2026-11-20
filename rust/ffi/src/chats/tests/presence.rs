//! Real native offline preflight and persisted privacy, without a network fixture.
use super::favorites::Fixture;
use super::offline::establish;
use super::*;

#[test]
fn native_presence_preflight_needs_a_due_match_before_network_access() {
    let f = Fixture::new();
    assert!(f.chats.presence_enabled().unwrap());
    assert!(!f.chats.announce_presence().unwrap());
    let peer = establish(&f);
    // An eligible match must reach active-network lookup rather than return a false success.
    assert_eq!(
        f.chats.announce_presence().unwrap_err().to_string(),
        "notConnected"
    );
    f.chats.set_presence_enabled(false).unwrap();
    assert!(!f.chats.presence_enabled().unwrap());
    assert!(!f.chats.announce_presence().unwrap());
    let reopened = f.reopen();
    assert!(!reopened.chats.presence_enabled().unwrap());
    assert!(!reopened.chats.announce_presence().unwrap());
    reopened.chats.set_presence_enabled(true).unwrap();
    assert_eq!(
        reopened.chats.announce_presence().unwrap_err().to_string(),
        "notConnected"
    );
    assert_eq!(
        reopened.chats.chat_history(peer).unwrap().state,
        ChatState::Open
    );
}

#[test]
fn native_hiding_own_presence_preserves_received_marks_and_other_identity() {
    let f = Fixture::new();
    let peer = establish(&f);
    let now = dyapp_client::clock::now_ms();
    let mut store = Store::open(
        &std::path::Path::new(&f.dir).join("client.db"),
        &f.key.identity,
    )
    .unwrap();
    assert!(store.receive_presence(&peer, now << 16, now).unwrap());
    drop(store);
    f.chats.set_presence_enabled(false).unwrap();
    assert_eq!(f.chats.active_chat_peers().unwrap(), vec![peer.clone()]);
    let other = Fixture::new();
    assert!(other.chats.presence_enabled().unwrap());
    assert_eq!(other.chats.active_chat_peers().unwrap().len(), 0);
    let reopened = f.reopen();
    assert_eq!(reopened.chats.active_chat_peers().unwrap(), vec![peer]);
}
