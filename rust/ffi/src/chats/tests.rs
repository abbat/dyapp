use dyapp_client::chat::MAX_TEXT_BYTES;

use super::*;
use crate::WrapError;
#[path = "tests/contact_notes.rs"]
mod contact_notes;
#[path = "tests/favorites.rs"]
mod favorites;
#[path = "tests/forwarding.rs"]
mod forwarding;
#[path = "tests/offline.rs"]
mod offline;

#[test]
fn a_full_node_gets_its_own_publish_code() {
    let full = publish::Failure::Node(dyapp_client::Error::Status(Status::Full));
    assert_eq!(publish_failed(full).to_string(), "full");
    let missed = publish_failed(publish::Failure::Missed).to_string();
    assert_eq!(missed, "the profile changed on another device");
}

/// XOR with a fixed byte: stands in for the platform keystore.
struct TestStore;

impl KeyStore for TestStore {
    fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
    }

    fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        self.wrap(wrapped)
    }
}

#[test]
fn the_first_open_stores_the_device_for_later_opens() {
    let key = IdentityKey::generate();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/native-chat-tests")
        .join(key.peer_id());
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.to_str().unwrap().to_owned();
    let device_key = IdentityKey::generate();
    let store: Arc<dyn KeyStore> = Arc::new(TestStore);
    let state_key = new_chat_state_key(store.clone()).unwrap();
    let open = |device_key: &Arc<IdentityKey>, state_key: &Vec<u8>| {
        ChatSession::open(
            dir.clone(),
            key.clone(),
            device_key.clone(),
            store.clone(),
            state_key.clone(),
        )
    };
    let chats = open(&device_key, &state_key).unwrap();
    let peer = "ab".repeat(32);
    check_offline_defaults(&chats);
    check_photo_controls(&chats, &dir, &peer);
    check_text_preflight(&chats, &key, &peer);
    check_empty_views(&chats, &peer);
    check_unknown_changes(&chats, &key, &peer);
    check_invalid_blocks(&chats, &key);
    check_blocked_views(&chats, &peer);
    check_preferences(&chats, &key, &peer);
    chats.block(peer).unwrap();
    // Publication is refused before network use.
    assert_eq!(
        chats.publish_profile(false).unwrap_err().to_string(),
        "blocked: notAttested"
    );
    drop(chats);

    // The stored snapshot opens again, under the same keys only.
    open(&device_key, &state_key).unwrap();
    let other_state = new_chat_state_key(store.clone()).unwrap();
    assert!(open(&device_key, &other_state).is_err());
    assert_eq!(
        open(&IdentityKey::generate(), &state_key)
            .err()
            .unwrap()
            .to_string(),
        "the device key changed"
    );
}

fn check_offline_defaults(chats: &ChatSession) {
    assert_eq!(chats.state("ab".repeat(32)).unwrap(), ChatState::None);
    assert_eq!(chats.receive().unwrap_err().to_string(), "notConnected");
    assert_eq!(chats.chat_rows().unwrap().len(), 0);
    assert_eq!(
        chats.photo_download().unwrap(),
        ChatPhotoDownload::ByNetwork
    );
    for mode in [
        ChatPhotoDownload::Always,
        ChatPhotoDownload::OnTap,
        ChatPhotoDownload::ByNetwork,
    ] {
        chats.set_photo_download(mode).unwrap();
        assert_eq!(chats.photo_download().unwrap(), mode);
    }
}

fn check_photo_controls(chats: &ChatSession, dir: &str, peer: &str) {
    // Cache controls remain offline and preserve the disabled quota across clearing.
    let cache = chats.set_media_cache(false, 393_216_123).unwrap();
    assert!(!cache.enabled);
    assert_eq!(cache.limit_bytes, 393_216_123);
    assert!(chats.set_media_cache(true, 0).is_err());
    let originals = std::path::Path::new(dir).join("originals");
    std::fs::create_dir(&originals).unwrap();
    std::fs::write(originals.join("cached"), b"download").unwrap();
    assert_eq!(chats.media_cache().unwrap().used_bytes, 8);
    assert_eq!(chats.clear_media_cache().unwrap(), cache);
    assert_eq!(chats.chat_rows().unwrap().len(), 0);
    // Even Always must keep a stranger behind a tap, regardless of network reports.
    chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_auto_download(dyapp_client::attachment::AutoDownload::Always)
        .unwrap();
    assert!(!chats.chat_photo_auto(peer.to_owned()).unwrap());
    chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_auto_download(dyapp_client::attachment::AutoDownload::ByNetwork)
        .unwrap();
}

fn send_refusal(chats: &ChatSession, to: &str, text: &str) -> String {
    chats
        .send_text(to.to_owned(), text.to_owned())
        .unwrap_err()
        .to_string()
}

fn edit_refusal(chats: &ChatSession, to: &str, id: Vec<u8>, text: &str) -> String {
    chats
        .edit_text(to.to_owned(), vec![1], id, text.to_owned())
        .unwrap_err()
        .to_string()
}

fn check_text_preflight(chats: &ChatSession, key: &IdentityKey, peer: &str) {
    assert_eq!(
        chats.like(peer.to_owned(), true).unwrap_err().to_string(),
        "unpublished"
    );
    assert_eq!(send_refusal(chats, &key.peer_id(), "hi"), "ownPeer");
    assert_eq!(send_refusal(chats, peer, ""), "invalidText");
    assert_eq!(
        send_refusal(chats, peer, &"x".repeat(MAX_TEXT_BYTES + 1)),
        "invalidText"
    );
    assert_eq!(send_refusal(chats, peer, "hi"), "unpublished");
    assert_eq!(
        send_refusal(chats, peer, &"x".repeat(MAX_TEXT_BYTES)),
        "unpublished"
    );
}

fn check_empty_views(chats: &ChatSession, peer: &str) {
    assert_eq!(
        chats.likes(peer.to_owned()).unwrap(),
        ChatLikes {
            ours: false,
            matched: false,
            passed: false
        }
    );
    assert_eq!(chats.liked_you().unwrap().len(), 0);
    assert!(chats.profiles(Vec::new()).unwrap().is_empty());
    assert_eq!(
        chats
            .profiles(vec![peer.to_owned()])
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
    assert_eq!(chats.chat_photos(peer.to_owned()).unwrap().len(), 0);
    assert_eq!(chats.chat_items(peer.to_owned()).unwrap().len(), 0);
    assert_eq!(
        chats
            .chat_photo_original(peer.to_owned(), vec![1], peer.to_owned(), vec![2; 16])
            .unwrap_err()
            .to_string(),
        "notFound"
    );
    let empty = chats.chat_history(peer.to_owned()).unwrap();
    assert_eq!((empty.state, empty.messages), (ChatState::None, Vec::new()));
}

fn check_unknown_changes(chats: &ChatSession, key: &IdentityKey, peer: &str) {
    // A chat with no state cannot be cleared or read; only a live own text can be edited.
    let err = |result: Result<u32, CoreError>| result.unwrap_err().to_string();
    assert_eq!(err(chats.clear_chat(peer.to_owned())), "unknownPeer");
    assert_eq!(err(chats.mark_read(peer.to_owned())), "unknownPeer");
    assert_eq!(err(chats.send_reads(peer.to_owned())), "unknownPeer");
    let name = |to: String| {
        chats
            .set_contact_name(to, "Bob".into())
            .unwrap_err()
            .to_string()
    };
    assert_eq!(name(peer.to_owned()), "unknownPeer");
    assert_eq!(name(key.peer_id()), "ownPeer");
    // Receipts are off until turned on.
    assert!(!chats.read_receipts().unwrap());
    chats.set_read_receipts(true).unwrap();
    assert!(chats.read_receipts().unwrap());
    assert_eq!(err(chats.clear_chat(key.peer_id())), "ownPeer");
    assert_eq!(edit_refusal(chats, peer, vec![2; 16], ""), "invalidText");
    assert_eq!(
        edit_refusal(chats, &key.peer_id(), vec![2; 16], "hi"),
        "ownPeer"
    );
    assert_eq!(
        edit_refusal(chats, peer, vec![2; 16], "hi"),
        "unknownMessage"
    );
    assert_eq!(
        edit_refusal(chats, peer, vec![2; 3], "hi"),
        "unknownMessage"
    );
    let delete = chats.delete_text(peer.to_owned(), vec![1], vec![2; 16]);
    assert_eq!(delete.unwrap_err().to_string(), "unknownMessage");
}

fn check_invalid_blocks(chats: &ChatSession, key: &IdentityKey) {
    let refused = |result: Result<ChatBlock, CoreError>| result.unwrap_err().to_string();
    assert_eq!(refused(chats.block(key.peer_id())), "invalidPeer");
    assert_eq!(refused(chats.block("ab".into())), "invalidPeer");
    assert_eq!(
        chats.unblock("xy".repeat(32)).unwrap_err().to_string(),
        "invalidPeer"
    );
}

fn check_blocked_views(chats: &ChatSession, peer: &str) {
    let err = |result: Result<u32, CoreError>| result.unwrap_err().to_string();
    // With no own devices and no issued tokens, a block is local and needs no revocation.
    assert_eq!(
        chats.block(peer.to_owned()).unwrap(),
        ChatBlock {
            revocation: ChatRevocation::NotNeeded,
            detail: None
        }
    );
    assert_eq!(send_refusal(chats, peer, "hi"), "blocked");
    assert_eq!(
        chats
            .chat_photo_auto(peer.to_owned())
            .unwrap_err()
            .to_string(),
        "blocked"
    );
    assert_eq!(err(chats.clear_chat(peer.to_owned())), "blocked");
    assert_eq!(edit_refusal(chats, peer, vec![2; 16], "hi"), "blocked");
    assert_eq!(
        chats.chat_history(peer.to_owned()).unwrap_err().to_string(),
        "blocked"
    );
    assert_eq!(
        chats.chat_photos(peer.to_owned()).unwrap_err().to_string(),
        "blocked"
    );
    assert_eq!(
        chats.chat_items(peer.to_owned()).unwrap_err().to_string(),
        "blocked"
    );
    assert_eq!(
        chats
            .chat_photo_original(peer.to_owned(), vec![1], peer.to_owned(), vec![2; 16])
            .unwrap_err()
            .to_string(),
        "blocked"
    );
    assert_eq!(chats.blocked_peers().unwrap(), std::slice::from_ref(&peer));
    chats.unblock(peer.to_owned()).unwrap();
    assert_eq!(chats.blocked_peers().unwrap().len(), 0);
}

fn check_preferences(chats: &ChatSession, key: &IdentityKey, peer: &str) {
    assert!(!chats.chat_history(peer.to_owned()).unwrap().muted);
    // A mute changes only the flag, and like a block it needs a valid other peer.
    chats.set_muted(peer.to_owned(), true).unwrap();
    assert!(chats.chat_history(peer.to_owned()).unwrap().muted);
    chats.set_muted(peer.to_owned(), false).unwrap();
    assert!(!chats.chat_history(peer.to_owned()).unwrap().muted);
    assert_eq!(
        chats
            .set_muted(key.peer_id(), true)
            .unwrap_err()
            .to_string(),
        "invalidPeer"
    );
    // A pass is local, shows in the like state and the list, and is undone the same way.
    assert!(chats.set_passed(peer.to_owned(), true).unwrap().passed);
    assert_eq!(chats.passed_peers().unwrap(), std::slice::from_ref(&peer));
    assert!(!chats.set_passed(peer.to_owned(), false).unwrap().passed);
    assert_eq!(chats.passed_peers().unwrap().len(), 0);
    let pass_self = chats.set_passed(key.peer_id(), true);
    assert_eq!(pass_self.unwrap_err().to_string(), "invalidPeer");
    // Off keeps the chosen content for the next turn-on.
    assert_eq!(chats.notice().unwrap(), ChatNotice::Fixed);
    chats.set_notice(ChatNotice::Text).unwrap();
    chats.set_notice(ChatNotice::Off).unwrap();
    assert_eq!(chats.notice().unwrap(), ChatNotice::Off);
    assert_eq!(chats.notice_content().unwrap(), ChatNotice::Text);
    chats.set_notice(ChatNotice::Name).unwrap();
    assert_eq!(chats.notice().unwrap(), ChatNotice::Name);
}
