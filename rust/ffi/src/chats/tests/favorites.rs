//! Exercise public exports through identity-bound native sessions without a network.
use super::*;
use crate::FavoriteError;

pub(super) struct Fixture {
    pub(super) chats: Arc<ChatSession>,
    pub(super) key: Arc<IdentityKey>,
    pub(super) device: Arc<IdentityKey>,
    pub(super) dir: String,
    pub(super) wrapped: Vec<u8>,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let key = IdentityKey::generate();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/native-favorites")
            .join(key.peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.to_str().unwrap().to_owned();
        let device = IdentityKey::generate();
        let wrapped = new_chat_state_key(Arc::new(TestStore)).unwrap();
        let chats = ChatSession::open(
            dir.clone(),
            key.clone(),
            device.clone(),
            Arc::new(TestStore),
            wrapped.clone(),
        )
        .unwrap();
        Self {
            chats,
            key,
            device,
            dir,
            wrapped,
        }
    }
}

#[test]
fn favorite_exports_are_offline_private_persistent_and_identity_bound() {
    let f = Fixture::new();
    let peer = IdentityKey::generate().peer_id();
    assert_eq!(f.chats.favorite_peers().unwrap(), Vec::<String>::new());
    assert!(!f.chats.favorite_state(peer.clone()).unwrap());
    f.chats.set_favorite(peer.clone(), true).unwrap();
    assert_eq!(f.chats.favorite_peers().unwrap(), vec![peer.clone()]);
    {
        let session = f.chats.session.lock().unwrap();
        assert!(session.store().due_outbound(0).unwrap().is_empty());
        assert_eq!(
            session.store().conversation(&peer).unwrap(),
            Conversation::None
        );
    }
    assert_eq!(
        Fixture::new().chats.favorite_peers().unwrap(),
        Vec::<String>::new()
    );
    drop(f.chats);
    let reopened =
        ChatSession::open(f.dir, f.key, f.device, Arc::new(TestStore), f.wrapped).unwrap();
    assert!(reopened.favorite_state(peer).unwrap());
}

#[test]
fn blocked_favorites_remain_readable_and_removable_with_typed_save_refusal() {
    let f = Fixture::new();
    let peer = IdentityKey::generate().peer_id();
    f.chats.set_favorite(peer.clone(), true).unwrap();
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, true)
        .unwrap();
    assert_eq!(f.chats.favorite_peers().unwrap(), Vec::<String>::new());
    assert!(f.chats.favorite_state(peer.clone()).unwrap());
    assert!(matches!(
        f.chats.set_favorite(peer.clone(), true),
        Err(FavoriteError::Blocked)
    ));
    f.chats.set_favorite(peer.clone(), false).unwrap();
    assert!(!f.chats.favorite_state(peer).unwrap());
}

#[test]
fn invalid_and_own_targets_are_typed_errors_without_writes() {
    let f = Fixture::new();
    assert!(matches!(
        f.chats.favorite_state("bad".into()),
        Err(FavoriteError::InvalidPeer)
    ));
    assert!(matches!(
        f.chats.set_favorite("bad".into(), true),
        Err(FavoriteError::InvalidPeer)
    ));
    assert!(matches!(
        f.chats.favorite_state(f.key.peer_id()),
        Err(FavoriteError::OwnPeer)
    ));
    assert!(matches!(
        f.chats.set_favorite(f.key.peer_id(), false),
        Err(FavoriteError::OwnPeer)
    ));
    assert_eq!(f.chats.favorite_peers().unwrap(), Vec::<String>::new());
}
