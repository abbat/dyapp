//! Actual native networking against deterministic replies; not production node or JNI coverage.
#[path = "photo_protocol/responder.rs"]
mod responder;

use super::favorites::Fixture;
use super::offline::{establish, snapshot};
use crate::{ChatPhoto, ChatSubmission};
use dyapp_client::attachment;
use dyapp_client::photo::prepare::{prepare_photo, PreparedPhoto, Purpose};
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::SignedRecord;
use libp2p::multiaddr::Protocol;
use prost::Message as _;
use responder::{Mode, Responder, Trace};
use std::path::Path;
use std::sync::Arc;

fn setup(mode: Mode) -> (Fixture, String, Responder, PreparedPhoto) {
    let f = Fixture::new();
    let peer = establish(&f);
    let (server, photo) = prepare_network(&f, mode);
    (f, peer, server, photo)
}

fn prepare_network(f: &Fixture, mode: Mode) -> (Responder, PreparedPhoto) {
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_kept_originals_mib(1)
        .unwrap();
    let server = Responder::start(mode, f.key.identity.public_key());
    let port = server
        .address
        .iter()
        .find_map(|part| match part {
            Protocol::Tcp(port) => Some(port),
            _ => None,
        })
        .unwrap();
    network::save(
        Path::new(&f.dir),
        Settings::Active,
        &Network {
            seeds: vec![format!("127.0.0.1:{port}")],
            name: String::new(),
        },
    )
    .unwrap();
    let photo = prepare_photo(&[255, 0, 0, 255], 1, 1, Purpose::Chat).unwrap();
    (server, photo)
}

fn submit(
    f: &Fixture,
    peer: &str,
    photo: &PreparedPhoto,
) -> Result<ChatSubmission, crate::CoreError> {
    f.chats.submit_chat_photo(
        peer.into(),
        photo.full.clone(),
        photo.thumbnail.clone(),
        photo.width,
        photo.height,
    )
}

fn only_photo(f: &Fixture, peer: &str, id: &[u8], trace: &Trace, expected: &[u8]) -> ChatPhoto {
    let rows = f.chats.chat_photos(peer.into()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    let session = f.chats.session.lock().unwrap();
    let stored = session.store().current_chat_attachments(peer).unwrap();
    assert_eq!(stored.len(), 1);
    let descriptor = &stored[0].message.attachment;
    assert_eq!(
        attachment::open(descriptor, &trace.blobs[0]).unwrap(),
        expected
    );
    assert_eq!(trace.attaches[0].release, descriptor.release_hash());
    rows.into_iter().next().unwrap()
}

fn ids(writes: &[Vec<u8>]) -> Vec<Vec<u8>> {
    writes
        .iter()
        .map(|write| {
            let record = SignedRecord::decode(write.as_slice()).unwrap();
            dyapp_p2p_net::proto::Envelope::decode(record.payload.as_slice())
                .unwrap()
                .id
        })
        .collect()
}

fn original(f: &Fixture, peer: &str, photo: &ChatPhoto) -> Vec<u8> {
    f.chats
        .chat_photo_original(
            peer.into(),
            photo.group_id.clone(),
            photo.author.clone(),
            photo.id.clone(),
        )
        .unwrap()
}

#[test]
fn upload_full_and_too_large_replies_do_not_commit_through_the_public_native_export() {
    for (mode, expected, steps) in [
        (Mode::AttachFull, "full", vec!["attach"]),
        (Mode::PutFull, "full", vec!["attach", "blob"]),
        (Mode::PutTooLarge, "tooLarge", vec!["attach", "blob"]),
    ] {
        let (f, peer, server, photo) = setup(mode);
        let before = snapshot(&f);
        let history = f.chats.chat_items(peer.clone()).unwrap();
        assert_eq!(submit(&f, &peer, &photo).unwrap_err().to_string(), expected);
        assert_eq!(snapshot(&f), before);
        assert_eq!(f.chats.chat_items(peer.clone()).unwrap(), history);
        assert_eq!(f.chats.chat_photos(peer).unwrap().len(), 0);
        let trace = server.finish();
        assert_eq!(trace.steps, steps);
        assert_eq!(trace.writes.len(), 0);
    }
}

#[test]
fn acknowledged_upload_then_mailbox_full_retains_one_id_and_exact_bytes_after_reopen() {
    let (f, peer, server, photo) = setup(Mode::MailboxFull);
    let invitation = snapshot(&f).1;
    let ChatSubmission::Queued { id, detail } = submit(&f, &peer, &photo).unwrap() else {
        panic!("mailbox refusal after upload must remain committed");
    };
    assert_eq!(detail, "full");
    let trace = server.finish();
    assert_eq!(trace.steps, ["attach", "blob", "mailbox"]);
    only_photo(&f, &peer, &id, &trace, &photo.full);
    accept_invitation_setup(&f, &invitation, &trace.writes[0]);
    let before = snapshot(&f);
    assert!(before.1.contains(&trace.writes[0]));
    let f = f.reopen();
    assert_eq!(snapshot(&f), before);
    let retried = Responder::start(Mode::RetryMailboxFull, f.key.identity.public_key());
    let retry_at = retry(&f, &retried, &trace.writes[0]);
    assert_eq!(snapshot(&f), before);
    let repeated = retried.finish();
    assert_eq!(repeated.writes.len(), 1);
    assert!(
        repeated.writes.contains(&trace.writes[0]),
        "retry_at={retry_at}, steps={:?}, expected={:?}, repeated={:?}, before={:?}",
        repeated.steps,
        ids(&trace.writes),
        ids(&repeated.writes),
        ids(&before.1)
    );
    assert!(repeated.writes.iter().all(|write| before.1.contains(write)));
    only_photo(&f, &peer, &id, &trace, &photo.full);
}

fn retry(f: &Fixture, server: &Responder, expected: &[u8]) -> i64 {
    crate::profile_network::on_network_stack(|| {
        let mut session = f.chats.session.lock().unwrap();
        let retry_at = f.chats.runtime.block_on(async {
            let mut node = dyapp_client::Node::connect(
                server.address.clone(),
                Arc::new(dyapp_p2p_net::Protocols::public()),
            )
            .await
            .unwrap();
            let retry_at = queued_deadline(session.store(), expected);
            session
                .retry_due(std::slice::from_mut(&mut node), retry_at, &f.chats.seal_key)
                .await
                .unwrap();
            retry_at
        });
        Ok(retry_at)
    })
    .unwrap()
}

fn accept_invitation_setup(f: &Fixture, invitation: &[Vec<u8>], photo: &[u8]) {
    let mut store =
        dyapp_client::store::Store::open(&Path::new(&f.dir).join("client.db"), &f.key.identity)
            .unwrap();
    let at = queued_deadline(&store, photo);
    // Only known fixture invitation writes become accepted setup; all records remain.
    for write in store.due_outbound(at).unwrap() {
        if invitation.iter().any(|bytes| bytes == write.as_bytes()) {
            store.accepted(&write, at, "fixture-establishment").unwrap();
        }
    }
    let kept = store.due_outbound(i64::MAX).unwrap();
    assert!(invitation
        .iter()
        .all(|bytes| kept.iter().any(|write| write.as_bytes() == bytes)));
}

fn queued_deadline(store: &dyapp_client::store::Store, expected: &[u8]) -> i64 {
    let due = |at| {
        store
            .due_outbound(at)
            .unwrap()
            .iter()
            .any(|write| write.as_bytes() == expected)
    };
    assert!(due(i64::MAX), "expected photo disappeared from the outbox");
    let (mut before, mut at) = (0, i64::MAX);
    while before < at {
        let middle = before + (at - before) / 2;
        if due(middle) {
            at = middle;
        } else {
            before = middle + 1;
        }
    }
    assert!(due(at));
    assert!(!due(at - 1), "photo was due before its selected boundary");
    at
}

#[test]
fn accepted_native_submission_downloads_its_exact_blob_and_reopens_cached_original_offline() {
    let (f, peer, server, photo) = setup(Mode::Accepted);
    let ChatSubmission::Accepted { id } = submit(&f, &peer, &photo).unwrap() else {
        panic!("all accepted writes must report accepted");
    };
    let item = only_photo(&f, &peer, &id, &server.trace(), &photo.full);
    assert_eq!(original(&f, &peer, &item), photo.full);
    let trace = server.finish();
    assert_eq!(&trace.steps[..2], ["attach", "blob"]);
    assert_eq!(trace.steps.last(), Some(&"get"));
    assert_ne!(trace.writes.len(), 0);
    let f = f.reopen();
    let before = snapshot(&f);
    assert_eq!(original(&f, &peer, &item), photo.full);
    assert_eq!(snapshot(&f), before);
    assert_eq!(f.chats.chat_photos(peer.clone()).unwrap().len(), 1);
    assert_photo_forward_preflight(&f, peer, item);
}

#[test]
fn native_forward_uploads_fresh_ciphertext_and_retains_the_source() {
    let f = Fixture::new();
    let source_peer = establish(&f);
    let (target_peer, profile) = super::offline::establish_with_profile(&f, [2; 16]);
    let (server, photo) = prepare_network(&f, Mode::Forwarding(profile));
    let ChatSubmission::Accepted { id: source_id } = submit(&f, &source_peer, &photo).unwrap()
    else {
        panic!("source submission was not accepted");
    };
    let initial = server.trace();
    assert_ne!(initial.writes.len(), 0);
    let item = only_photo(&f, &source_peer, &source_id, &initial, &photo.full);
    let source = crate::ChatForwardSource {
        peer_id: source_peer.clone(),
        group_id: item.group_id,
        author: item.author,
        message_id: source_id.clone(),
    };
    let ChatSubmission::Accepted { id } =
        f.chats.forward_photo(source, target_peer.clone()).unwrap()
    else {
        panic!("forwarded submission was not accepted");
    };
    assert_ne!(id, source_id);
    let rows = f.chats.chat_photos(target_peer.clone()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    assert!(rows[0].forwarded);
    assert_eq!(original(&f, &target_peer, &rows[0]), photo.full);
    let trace = server.finish();
    assert!(trace.writes.len() > initial.writes.len());
    assert_eq!(trace.attaches.len(), 2);
    assert_eq!(trace.blobs.len(), 2);
    assert_ne!(trace.attaches[0].hashes, trace.attaches[1].hashes);
    assert_ne!(trace.attaches[0].release, trace.attaches[1].release);
    let media_steps: Vec<_> = trace
        .steps
        .iter()
        .copied()
        .filter(|step| *step != "mailbox")
        .collect();
    assert_eq!(
        media_steps,
        ["attach", "blob", "profile", "get", "attach", "blob", "get"]
    );
    assert!(!only_photo(&f, &source_peer, &source_id, &trace, &photo.full).forwarded);
}

fn assert_photo_forward_preflight(f: &Fixture, peer: String, item: ChatPhoto) {
    let target = super::offline::establish_with_event(f, [2; 16]);
    let source = crate::ChatForwardSource {
        peer_id: peer,
        group_id: item.group_id,
        author: item.author,
        message_id: item.id,
    };
    let before = snapshot(f);
    assert_eq!(
        f.chats.forward_photo_targets(source.clone()).unwrap(),
        vec![target.clone()]
    );
    let mut wrong = source.clone();
    wrong.group_id.clear();
    assert!(matches!(
        f.chats.forward_photo_targets(wrong),
        Err(crate::ChatForwardError::UnknownMessage)
    ));
    assert_eq!(snapshot(f), before);
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&target, true)
        .unwrap();
    let blocked = snapshot(f);
    assert!(matches!(
        f.chats.forward_photo(source, target),
        Err(crate::ChatForwardError::Blocked)
    ));
    assert_eq!(snapshot(f), blocked);
}
