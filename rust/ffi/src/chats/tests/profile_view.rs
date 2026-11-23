//! Exercise real native opening admission without an active network or a published own profile.
use super::favorites::Fixture;
use super::*;

fn target(f: &Fixture) -> Arc<ProfileViewOpening> {
    f.chats
        .profile_view_opening(IdentityKey::generate().unwrap().peer_id())
        .unwrap()
}

#[test]
fn disabled_and_retired_openings_do_not_require_publication_or_network() {
    let f = Fixture::new();
    let disabled = target(&f);
    assert!(f
        .chats
        .submit_profile_view(disabled.clone())
        .unwrap()
        .is_none());
    f.chats.set_profile_views_enabled(true).unwrap();
    assert!(f.chats.submit_profile_view(disabled).unwrap().is_none());
    let retired = target(&f);
    retired.retire();
    assert!(f.chats.submit_profile_view(retired).unwrap().is_none());
    // A fresh enabled opening reaches actual network lookup; this is not a false success.
    assert_eq!(
        f.chats
            .submit_profile_view(target(&f))
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
    let session = f.chats.session.lock().unwrap();
    assert_eq!(session.store().known_groups().unwrap().len(), 0);
    assert_eq!(session.store().due_outbound(i64::MAX).unwrap().len(), 0);
}

#[test]
fn replaced_context_and_other_identity_cannot_retarget_an_opening() {
    let f = Fixture::new();
    let other = Fixture::new();
    f.chats.set_profile_views_enabled(true).unwrap();
    other.chats.set_profile_views_enabled(true).unwrap();
    let original = target(&f);
    assert_eq!(
        other
            .chats
            .submit_profile_view(original.clone())
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
    f.chats.contact_context.lock().unwrap().clear();
    assert_eq!(
        f.chats
            .submit_profile_view(original)
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
    // Switching back cannot restore a captured generation.
    let old = target(&f);
    f.chats.contact_context.lock().unwrap().clear();
    f.chats.contact_context.lock().unwrap().clear();
    assert_eq!(
        f.chats.submit_profile_view(old).unwrap_err().to_string(),
        "notConnected"
    );
}

#[test]
fn search_context_openings_are_bound_to_the_actual_session_owner() {
    let f = Fixture::new();
    f.chats.set_profile_views_enabled(true).unwrap();
    let searcher = crate::ProfileSearcher::new(f.dir.clone()).unwrap();
    let peer = IdentityKey::generate().unwrap().peer_id();
    let wrong = searcher
        .profile_view_opening(IdentityKey::generate().unwrap().peer_id(), peer.clone())
        .unwrap();
    assert_eq!(
        f.chats.submit_profile_view(wrong).unwrap_err().to_string(),
        "notConnected"
    );
    let original = searcher
        .profile_view_opening(f.key.peer_id(), peer)
        .unwrap();
    f.chats.contact_context.lock().unwrap().clear();
    assert_eq!(
        f.chats
            .submit_profile_view(original)
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
}

#[test]
fn opening_capture_finishes_while_contact_io_keeps_the_context_mutex() {
    use std::sync::mpsc;
    use std::time::Duration;

    let f = Fixture::new();
    f.chats.set_profile_views_enabled(true).unwrap();
    let searcher = crate::ProfileSearcher::new(f.dir.clone()).unwrap();
    assert!(Arc::ptr_eq(&searcher.cache, &f.chats.contact_context));
    let owner = f.key.peer_id();
    let peer = IdentityKey::generate().unwrap().peer_id();
    let (search, chat) = std::thread::scope(|scope| {
        let context = Arc::clone(&f.chats.contact_context);
        let (ready, entered) = mpsc::channel();
        let (release, released) = mpsc::channel();
        scope.spawn(move || {
            let _held = context.lock().unwrap();
            ready.send(()).unwrap();
            // Driver success/failure releases this gate; no elapsed delay admits capture.
            let _ = released.recv();
        });
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        let chats = Arc::clone(&f.chats);
        let (finish, finished) = mpsc::channel();
        let capture = scope.spawn(move || {
            let search = searcher.profile_view_opening(owner, peer.clone());
            let chat = chats.profile_view_opening(peer);
            finish.send((search, chat)).unwrap();
        });
        let result = finished.recv_timeout(Duration::from_secs(10));
        let _ = release.send(());
        capture.join().unwrap();
        result.expect("both factories must complete before the contact IO lock is released")
    });
    f.chats.contact_context.lock().unwrap().clear();
    for opening in [search.unwrap(), chat.unwrap()] {
        assert_eq!(
            f.chats
                .submit_profile_view(opening)
                .unwrap_err()
                .to_string(),
            "notConnected"
        );
    }
}

#[test]
fn invalid_own_and_blocked_targets_are_refused_before_network_access() {
    let f = Fixture::new();
    f.chats.set_profile_views_enabled(true).unwrap();
    let own = f.chats.profile_view_opening(f.key.peer_id()).unwrap();
    assert_eq!(
        f.chats.submit_profile_view(own).unwrap_err().to_string(),
        "ownPeer"
    );
    let invalid = f.chats.profile_view_opening("invalid".into()).unwrap();
    assert_eq!(
        f.chats
            .submit_profile_view(invalid)
            .unwrap_err()
            .to_string(),
        "invalidPeer"
    );
    let peer = IdentityKey::generate().unwrap().peer_id();
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&peer, true)
        .unwrap();
    let blocked = f.chats.profile_view_opening(peer).unwrap();
    assert_eq!(
        f.chats
            .submit_profile_view(blocked)
            .unwrap_err()
            .to_string(),
        "client store peer is blocked"
    );
}
