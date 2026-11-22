use super::*;

async fn apply_peer_change(
    session: &mut Session,
    peer: &str,
    change: crate::chat::PeerChange,
    seal: &[u8; 32],
) {
    session
        .change_peer(&mut [], peer, change, seal)
        .await
        .unwrap();
}

#[tokio::test]
async fn private_changes_without_own_devices_create_no_peer_message() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let (owner, device, own_profile) = person();
    let (_, contact, profile) = person();
    let peer = profile.peer_id.clone();
    let (group, _) = device
        .create_group(
            &[contact.key_package(false).unwrap()],
            &[own_profile, profile.clone()],
        )
        .unwrap();
    let seal = [9; 32];
    let mut store = Store::open(&test_dir.path().join("client-1.db"), &owner).unwrap();
    store
        .stage_group(&group, &profile, &device.seal(&seal).unwrap(), &[])
        .unwrap();
    let before = store.conversation(&peer).unwrap();
    let mut session = Session::open(store, &seal).unwrap();
    apply_peer_change(
        &mut session,
        &peer,
        crate::chat::PeerChange::Favorite(true),
        &seal,
    )
    .await;
    assert!(session.store().favorite(&peer).unwrap());
    assert!(session.store().due_outbound(0).unwrap().is_empty());
    assert_eq!(session.store().conversation(&peer).unwrap(), before);
    apply_peer_change(
        &mut session,
        &peer,
        crate::chat::PeerChange::Favorite(false),
        &seal,
    )
    .await;
    assert!(!session.store().favorite(&peer).unwrap());
    for note in ["Private\nNote", ""] {
        apply_peer_change(
            &mut session,
            &peer,
            crate::chat::PeerChange::Note(note.into()),
            &seal,
        )
        .await;
        assert_eq!(session.store().contact_note(&peer).unwrap(), note);
        assert!(session.store().due_outbound(0).unwrap().is_empty());
        assert_eq!(session.store().conversation(&peer).unwrap(), before);
    }
}

#[test]
fn only_a_newer_profile_without_this_device_revokes_it() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let (identity, device, _) = person();
    let seal_key = [3; 32];
    let mut store = Store::open(&test_dir.path().join("client-1.db"), &identity).unwrap();
    store
        .stage_outbound(&device.seal(&seal_key).unwrap(), &[])
        .unwrap();
    let session = Session::open(store, &seal_key).unwrap();
    let certificate = device.certificate().clone();
    let binding = crate::owner_binding(&Identity::try_generate().unwrap(), &identity.public_key());
    let without = |version| Profile {
        version,
        ..Profile::default()
    };
    let revoked = |profile: &Profile| session.own_device_revoked(profile).unwrap();

    // A new device was never listed, whatever the node holds.
    assert!(!revoked(&without(5)));
    let mut draft = without(5);
    session.list_self(&mut draft, binding.clone()).unwrap();
    assert_eq!(draft.devices, vec![certificate.clone()]);

    // Once listed, neither the same version nor an older copy drops it.
    assert!(!revoked(&draft));
    assert_eq!(session.store().device_listed().unwrap(), 5);
    assert!(!revoked(&without(5)));
    assert!(!revoked(&without(4)));
    assert!(revoked(&without(6)));

    // A withdrawn profile carries no device at all.
    let withdrawn = Profile {
        deleted: true,
        ..without(6)
    };
    assert!(!revoked(&withdrawn));

    // A draft taken from the profile that dropped it cannot bring it back.
    let mut draft = without(6);
    assert!(matches!(
        session.list_self(&mut draft, binding),
        Err(Error::Revoked)
    ));
    assert_eq!(draft.devices, [] as [dyapp_identity::SignedRecord; 0]);
}

struct OutboundFixture {
    session: Session,
    path: std::path::PathBuf,
    alice_id: Identity,
    bob_profile: dyapp_profile::VerifiedProfile,
    group_id: Vec<u8>,
    seal_key: [u8; 32],
    _test_dir: dyapp_test_support::TestDir,
}

impl OutboundFixture {
    fn reopen(&mut self) {
        self.session = Session::open(
            Store::open(&self.path, &self.alice_id).unwrap(),
            &self.seal_key,
        )
        .unwrap();
    }
}

struct StagedMessages {
    writes: Vec<crate::PreparedEnvelope>,
    offline_writes: Vec<crate::PreparedEnvelope>,
    forward_writes: Vec<crate::PreparedEnvelope>,
    offline_id: [u8; 16],
    forward_id: [u8; 16],
    own: MessageKey,
}

fn outbound_fixture() -> OutboundFixture {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let (alice_id, alice, alice_profile) = person();
    let (_, bob, bob_profile) = person();
    let (group, _) = alice
        .create_group(
            &[bob.key_package(false).unwrap()],
            &[alice_profile, bob_profile.clone()],
        )
        .unwrap();
    let seal_key = [3; 32];
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = test_dir.path().join(format!(
        "dyapp-send-session-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let mut store = Store::open(&path, &alice_id).unwrap();
    store
        .stage_group(&group, &bob_profile, &alice.seal(&seal_key).unwrap(), &[])
        .unwrap();
    // A match opens the chat, so the second text is not a second pending request.
    let alice_peer = alice_id.peer_id();
    let like = store
        .interact(
            &bob_profile.peer_id,
            crate::interactions::Kind::Like,
            &[1],
            1_000,
        )
        .unwrap()
        .unwrap();
    store
        .apply_interaction(&bob_profile.peer_id, &alice_peer, &[2], like)
        .unwrap();
    let session = Session::open(store, &seal_key).unwrap();
    OutboundFixture {
        session,
        path,
        alice_id,
        bob_profile,
        group_id: group.id().to_vec(),
        seal_key,
        _test_dir: test_dir,
    }
}

fn assert_message_projection(
    session: &Session,
    fixture: &OutboundFixture,
    staged: &StagedMessages,
) {
    let sent = session
        .store
        .current_chat_messages(&fixture.bob_profile.peer_id)
        .unwrap();
    let forwarded = sent
        .iter()
        .find(|message| message.key.id == staged.forward_id)
        .unwrap();
    assert!(forwarded.forwarded);
    assert!(forwarded.original_hlc.is_some());
    assert_eq!(
        forwarded.current.text.as_deref(),
        Some(&b"forwarded words"[..])
    );
    let original_hlc = sent
        .iter()
        .find(|message| message.key.id == staged.offline_id)
        .unwrap()
        .original_hlc
        .unwrap();
    assert!(u64::from(original_hlc) > 0);
    assert!(session.store.draft_chat_text(crate::now() * 1000).unwrap() > u64::from(original_hlc));
    assert_eq!(
        session
            .store
            .chat_message(&staged.own)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"offline"[..])
    );
}

fn assert_initial_due(session: &Session, staged: &StagedMessages) {
    let due = session.due_outbound(0).unwrap();
    for writes in [
        &staged.writes,
        &staged.offline_writes,
        &staged.forward_writes,
    ] {
        assert!(due
            .iter()
            .any(|write| write.as_bytes() == writes[0].as_bytes()));
    }
}

fn stage_messages(fixture: &mut OutboundFixture) -> StagedMessages {
    let writes = fixture
        .session
        .stage_message(
            &fixture.group_id,
            &Body::text_with_id([1; 16], b"hello".to_vec()),
            &fixture.seal_key,
        )
        .unwrap();
    assert_eq!(writes.len(), 1);
    let (offline_id, offline_writes) = fixture
        .session
        .stage_text(&fixture.group_id, b"offline", &fixture.seal_key)
        .unwrap();
    assert_ne!(offline_id, [1; 16]);
    assert_eq!(offline_writes.len(), 1);
    let (forward_id, forward_writes) = fixture
        .session
        .stage_forwarded_text(&fixture.group_id, b"forwarded words", &fixture.seal_key)
        .unwrap();
    assert_ne!(forward_id, offline_id);
    assert_eq!(forward_writes.len(), 1);
    let own = MessageKey {
        group: fixture.group_id.clone(),
        author: fixture.alice_id.peer_id(),
        id: offline_id,
    };
    let staged = StagedMessages {
        writes,
        offline_writes,
        forward_writes,
        offline_id,
        forward_id,
        own,
    };
    assert_message_projection(&fixture.session, fixture, &staged);
    assert_initial_due(&fixture.session, &staged);
    staged
}

fn assert_reopened_messages(fixture: &OutboundFixture, staged: &StagedMessages) {
    assert!(fixture
        .session
        .store
        .current_chat_messages(&fixture.bob_profile.peer_id)
        .unwrap()
        .iter()
        .any(|message| message.key.id == staged.forward_id && message.forwarded));
    assert_eq!(
        fixture
            .session
            .chat_message(&staged.own)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"offline"[..])
    );
}

fn reject_invalid_edits(fixture: &mut OutboundFixture, staged: &StagedMessages) {
    let before_invalid = fixture.session.store.sealed_device().unwrap();
    assert!(matches!(
        fixture
            .session
            .stage_edit(&[9], staged.offline_id, b"wrong group", &fixture.seal_key),
        Err(Error::UnknownMessage)
    ));
    assert!(matches!(
        fixture
            .session
            .stage_delete(&fixture.group_id, [9; 16], &fixture.seal_key),
        Err(Error::UnknownMessage)
    ));
    assert_eq!(
        fixture.session.store.sealed_device().unwrap(),
        before_invalid
    );
}

fn edit_and_delete(
    fixture: &mut OutboundFixture,
    staged: &StagedMessages,
) -> (Vec<crate::PreparedEnvelope>, Vec<crate::PreparedEnvelope>) {
    let (edit_id, edit_writes) = fixture
        .session
        .stage_edit(
            &fixture.group_id,
            staged.offline_id,
            b"revised",
            &fixture.seal_key,
        )
        .unwrap();
    assert_ne!(edit_id, [0; 16]);
    assert_eq!(
        fixture
            .session
            .chat_message(&staged.own)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"revised"[..])
    );
    let (delete_id, delete_writes) = fixture
        .session
        .stage_delete(&fixture.group_id, staged.offline_id, &fixture.seal_key)
        .unwrap();
    assert_ne!(delete_id, edit_id);
    assert!(matches!(
        fixture.session.stage_edit(
            &fixture.group_id,
            staged.offline_id,
            b"restore",
            &fixture.seal_key
        ),
        Err(Error::UnknownMessage)
    ));
    let current = fixture.session.chat_message(&staged.own).unwrap().unwrap();
    assert!(current.edited && current.deleted && current.text.is_none());
    (edit_writes, delete_writes)
}

fn restart_and_edit(
    fixture: &mut OutboundFixture,
    staged: &StagedMessages,
) -> (Vec<crate::PreparedEnvelope>, Vec<crate::PreparedEnvelope>) {
    fixture.reopen();
    assert_reopened_messages(fixture, staged);
    reject_invalid_edits(fixture, staged);
    edit_and_delete(fixture, staged)
}

#[test]
fn message_is_staged_before_network_and_reopens_exact_bytes() {
    let mut fixture = outbound_fixture();
    let staged = stage_messages(&mut fixture);
    let (edit_writes, delete_writes) = restart_and_edit(&mut fixture, &staged);
    fixture.reopen();
    assert!(
        fixture
            .session
            .chat_message(&staged.own)
            .unwrap()
            .unwrap()
            .deleted
    );
    let due = fixture.session.due_outbound(0).unwrap();
    for exact in [
        &staged.writes[0],
        &staged.offline_writes[0],
        &edit_writes[0],
        &delete_writes[0],
    ] {
        assert!(due.iter().any(|write| write.as_bytes() == exact.as_bytes()));
    }
    assert_eq!(fixture.session.clear_chat(&fixture.group_id).unwrap(), 3);
    assert!(fixture.session.chat_message(&staged.own).unwrap().is_none());
    assert!(fixture.session.due_outbound(0).unwrap().is_empty());
}

#[test]
fn only_a_resolved_profile_sends_the_next_page_to_the_head() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let (identity, device_key) = (
        Identity::try_generate().unwrap(),
        Identity::try_generate().unwrap(),
    );
    let store = Store::open(&test_dir.path().join("client-1.db"), &identity).unwrap();
    let session = Session::open_or_create(store, &identity, &device_key, &[5; 32]).unwrap();
    let node = libp2p::PeerId::random();
    session.store.save_mailbox_cursor(node, &[1]).unwrap();

    assert!(!session.next_page(false, false).unwrap());
    assert!(session.next_page(true, false).unwrap());
    assert_eq!(
        session.store.mailbox_cursor_for(node).unwrap(),
        Some(vec![1])
    );

    assert!(session.next_page(false, true).unwrap());
    assert_eq!(session.store.mailbox_cursor_for(node).unwrap(), None);
}
