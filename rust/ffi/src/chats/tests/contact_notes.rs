//! Public native notes preserve exact private text and distinguish empty from errors.
use super::favorites::Fixture;
use super::*;
use crate::ContactNoteError;

#[test]
fn failed_note_access_is_an_error_instead_of_an_empty_note() {
    let f = Fixture::new();
    let peer = "ab".repeat(32);
    assert_eq!(f.chats.contact_note(peer.clone()).unwrap(), "");
    let chats = Arc::clone(&f.chats);
    let poison = std::thread::spawn(move || {
        let _guard = chats.session.lock().unwrap();
        panic!("poison isolated note fixture");
    });
    assert!(poison.join().is_err());
    assert!(matches!(
        f.chats.contact_note(peer.clone()),
        Err(ContactNoteError::Failed { .. })
    ));
    assert!(matches!(
        f.chats.set_contact_note(peer, String::new()),
        Err(ContactNoteError::Failed { .. })
    ));
}

#[test]
fn uppercase_private_targets_cannot_bypass_identity_policy_or_create_aliases() {
    let f = Fixture::new();
    let peer = "ab".repeat(32);
    f.chats
        .set_contact_note(peer.clone(), "keep".into())
        .unwrap();
    f.chats.set_favorite(peer.clone(), true).unwrap();
    for target in [
        peer.to_ascii_uppercase(),
        f.key.peer_id().to_ascii_uppercase(),
    ] {
        assert!(matches!(
            f.chats.set_contact_note(target.clone(), "alias".into()),
            Err(ContactNoteError::InvalidPeer | ContactNoteError::OwnPeer)
        ));
        assert!(matches!(
            f.chats.contact_note(target.clone()),
            Err(ContactNoteError::InvalidPeer | ContactNoteError::OwnPeer)
        ));
        assert!(matches!(
            f.chats.set_favorite(target.clone(), true),
            Err(crate::FavoriteError::InvalidPeer | crate::FavoriteError::OwnPeer)
        ));
        assert!(matches!(
            f.chats.favorite_state(target),
            Err(crate::FavoriteError::InvalidPeer | crate::FavoriteError::OwnPeer)
        ));
    }
    assert_eq!(f.chats.contact_note(peer.clone()).unwrap(), "keep");
    assert_eq!(f.chats.favorite_peers().unwrap(), vec![peer.clone()]);
    let session = f.chats.session.lock().unwrap();
    assert_eq!(
        session
            .store()
            .contact_note(&peer.to_ascii_uppercase())
            .unwrap(),
        ""
    );
    assert!(!session
        .store()
        .favorite(&peer.to_ascii_uppercase())
        .unwrap());
}

#[test]
fn notes_preserve_text_across_reopen_without_peer_messages_or_identity_leaks() {
    let f = Fixture::new();
    let peer = IdentityKey::generate().peer_id();
    let text = "Case\nПриватная заметка\n😀";
    assert_eq!(f.chats.contact_note(peer.clone()).unwrap(), "");
    f.chats.set_contact_note(peer.clone(), text.into()).unwrap();
    assert_eq!(f.chats.contact_note(peer.clone()).unwrap(), text);
    {
        let session = f.chats.session.lock().unwrap();
        assert!(session.store().due_outbound(0).unwrap().is_empty());
        assert_eq!(
            session.store().conversation(&peer).unwrap(),
            Conversation::None
        );
    }
    assert_eq!(Fixture::new().chats.contact_note(peer.clone()).unwrap(), "");
    drop(f.chats);
    let reopened =
        ChatSession::open(f.dir, f.key, f.device, Arc::new(TestStore), f.wrapped).unwrap();
    assert_eq!(reopened.contact_note(peer).unwrap(), text);
}

#[test]
fn blocked_contacts_keep_note_access_and_empty_text_is_an_explicit_clear() {
    let f = Fixture::new();
    let peer = IdentityKey::generate().peer_id();
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, true)
        .unwrap();
    f.chats
        .set_contact_note(peer.clone(), "private".into())
        .unwrap();
    assert_eq!(f.chats.contact_note(peer.clone()).unwrap(), "private");
    f.chats
        .set_contact_note(peer.clone(), String::new())
        .unwrap();
    assert_eq!(f.chats.contact_note(peer).unwrap(), "");
}

#[test]
fn invalid_text_or_targets_are_typed_errors_and_preserve_the_existing_note() {
    let f = Fixture::new();
    let peer = IdentityKey::generate().peer_id();
    f.chats
        .set_contact_note(peer.clone(), "keep".into())
        .unwrap();
    for note in [
        "😀".repeat(dyapp_client::chat::MAX_NOTE_CHARS + 1),
        "hidden\u{202e}".into(),
        "tab\t".into(),
        "e\u{301}".into(),
    ] {
        assert!(matches!(
            f.chats.set_contact_note(peer.clone(), note),
            Err(ContactNoteError::InvalidText)
        ));
        assert_eq!(f.chats.contact_note(peer.clone()).unwrap(), "keep");
    }
    assert!(matches!(
        f.chats.contact_note("bad".into()),
        Err(ContactNoteError::InvalidPeer)
    ));
    assert!(matches!(
        f.chats.set_contact_note("bad".into(), String::new()),
        Err(ContactNoteError::InvalidPeer)
    ));
    assert!(matches!(
        f.chats.contact_note(f.key.peer_id()),
        Err(ContactNoteError::OwnPeer)
    ));
    assert!(matches!(
        f.chats.set_contact_note(f.key.peer_id(), String::new()),
        Err(ContactNoteError::OwnPeer)
    ));
}
