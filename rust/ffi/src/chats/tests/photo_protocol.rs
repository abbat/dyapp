//! Actual native networking against deterministic replies; not production node or JNI coverage.
#[path = "photo_protocol/responder.rs"]
mod responder;

use super::favorites::Fixture;
use super::offline::{establish, snapshot};
use crate::{ChatPhoto, ChatSubmission};
use dyapp_client::attachment;
use dyapp_client::photo::prepare::{prepare_photo, PreparedPhoto, Purpose};
use dyapp_identity::network::{self, Network, Settings};
use libp2p::multiaddr::Protocol;
use responder::{Mode, Responder, Trace};
use std::path::Path;
use std::sync::Arc;

fn setup(mode: Mode) -> (Fixture, String, Responder, PreparedPhoto) {
    let f = Fixture::new();
    let peer = establish(&f);
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
    (f, peer, server, photo)
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
    let ChatSubmission::Queued { id, detail } = submit(&f, &peer, &photo).unwrap() else {
        panic!("mailbox refusal after upload must remain committed");
    };
    assert_eq!(detail, "full");
    let trace = server.finish();
    assert_eq!(trace.steps, ["attach", "blob", "mailbox"]);
    only_photo(&f, &peer, &id, &trace, &photo.full);
    let before = snapshot(&f);
    assert!(before.1.contains(&trace.writes[0]));
    let f = f.reopen();
    assert_eq!(snapshot(&f), before);
    let retried = Responder::start(Mode::RetryMailboxFull, f.key.identity.public_key());
    retry(&f, &retried.address);
    assert_eq!(snapshot(&f), before);
    let repeated = retried.finish();
    assert!(repeated.writes.contains(&trace.writes[0]));
    assert!(repeated.writes.iter().all(|write| before.1.contains(write)));
    only_photo(&f, &peer, &id, &trace, &photo.full);
}

fn retry(f: &Fixture, address: &libp2p::Multiaddr) {
    crate::profile_network::on_network_stack(|| {
        let mut session = f.chats.session.lock().unwrap();
        f.chats.runtime.block_on(async {
            let node = dyapp_client::Node::connect(
                address.clone(),
                Arc::new(dyapp_p2p_net::Protocols::public()),
            )
            .await
            .unwrap();
            let now = i64::try_from(dyapp_client::now() * 1000).unwrap();
            session
                .retry_due(&mut [node], now, &f.chats.seal_key)
                .await
                .unwrap();
        });
        Ok(())
    })
    .unwrap();
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
    assert_eq!(f.chats.chat_photos(peer).unwrap().len(), 1);
}
