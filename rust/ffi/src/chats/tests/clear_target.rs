use super::favorites::Fixture;
use super::*;

fn known(f: &Fixture, peer: &str) {
    let db = rusqlite::Connection::open(std::path::Path::new(&f.dir).join("client.db")).unwrap();
    db.execute(
        "INSERT INTO `conversations` (`peer_id`, `state`, `request_id`) VALUES (?1, 3, NULL)",
        [peer],
    )
    .unwrap();
}

#[test]
fn native_clear_target_is_read_only_owner_bound_and_stable_after_reopen() {
    let f = Fixture::new();
    let peer = IdentityKey::generate().unwrap().peer_id();
    known(&f, &peer);
    let target = f.chats.clear_target(peer.clone()).unwrap();
    assert_eq!(target.owner_peer_id, f.key.peer_id());
    assert_eq!(target.peer_id, peer);
    assert!(f.chats.chat_items(peer.clone()).unwrap().is_empty());
    assert!(f
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .due_outbound(0)
        .unwrap()
        .is_empty());
    let f = f.reopen();
    assert_eq!(f.chats.clear_target(peer.clone()).unwrap(), target);
    assert_eq!(f.chats.clear_chat(peer).unwrap(), 0);
}

#[test]
fn target_refusals_and_later_clear_revalidation_use_the_same_common_policy() {
    let f = Fixture::new();
    let peer = IdentityKey::generate().unwrap().peer_id();
    assert_eq!(
        f.chats
            .clear_target(f.key.peer_id())
            .unwrap_err()
            .to_string(),
        "ownPeer"
    );
    assert_eq!(
        f.chats.clear_target(peer.clone()).unwrap_err().to_string(),
        "unknownPeer"
    );
    known(&f, &peer);
    let held = f.chats.clear_target(peer.clone()).unwrap();
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, true)
        .unwrap();
    assert_eq!(
        f.chats.clear_target(peer.clone()).unwrap_err().to_string(),
        "blocked"
    );
    assert_eq!(
        f.chats.clear_chat(held.peer_id).unwrap_err().to_string(),
        "blocked"
    );
    assert_eq!(
        Fixture::new()
            .chats
            .clear_target(peer)
            .unwrap_err()
            .to_string(),
        "unknownPeer"
    );
}
