//! Source selection plus real MLS staging preserves the forwarded commitment on delivery failure.
use super::*;
use crate::forwarding::tests::open_chat;
use crate::forwarding::{self, Source};

#[tokio::test]
async fn selected_forward_keeps_forwarder_identity_and_exact_queued_bytes_after_reopen() {
    let mut f = fixture();
    let source = source_message(&mut f);
    let source_peer = source.author.clone();
    open_chat(&mut f.session.store, &source_peer);
    open_chat(&mut f.session.store, &f.peer);
    let text = forwarding::prepare(
        f.session.store(),
        Source {
            peer: &source_peer,
            group: &source.group,
            author: &source.author,
            id: &source.id,
        },
        &f.peer,
    )
    .unwrap();
    let outcome = f
        .session
        .submit_forwarded_text_to(&mut [], &f.peer, &text, &f.seal)
        .await
        .unwrap();
    let id = outcome.id();
    assert!(matches!(outcome, Submission::Queued { .. }));
    check_forward(&f.session, &f.peer, &f.owner.peer_id(), id);
    let before = writes(&f.session);
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    assert_eq!(writes(&reopened), before);
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), before);
    check_forward(&reopened, &f.peer, &f.owner.peer_id(), id);
    assert_eq!(
        reopened
            .chat_message(&source)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(b"selected words".as_slice())
    );
}

fn source_message(f: &mut Fixture) -> MessageKey {
    let (_, source_device, profile) = person();
    let profiles = [f.profiles[0].clone(), profile.clone()];
    let device = f.session.device.as_ref().unwrap();
    let (group, _) = device
        .create_group(&[source_device.key_package(false).unwrap()], &profiles)
        .unwrap();
    let source = MessageKey {
        group: group.id().to_vec(),
        author: profile.peer_id.clone(),
        id: [9; 16],
    };
    f.session
        .store
        .stage_group(&group, &profile, &device.seal(&f.seal).unwrap(), &[])
        .unwrap();
    f.session.groups.push(group);
    f.session
        .store
        .record_chat_text(&source, b"selected words")
        .unwrap();
    source
}

fn check_forward(session: &Session, peer: &str, owner: &str, id: [u8; 16]) {
    let rows = session.store().current_chat_messages(peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    assert_eq!(rows[0].key.author, owner);
    assert!(rows[0].forwarded);
    assert_eq!(
        rows[0].current.text.as_deref(),
        Some(b"selected words".as_slice())
    );
}
