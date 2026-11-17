//! Public native refusal boundaries; real node upload and device/JNI execution remain separate.
use super::favorites::Fixture;
use super::offline::{establish, snapshot};
use crate::IdentityKey;
use dyapp_client::photo::prepare::{prepare_photo, PreparedPhoto, Purpose};
use dyapp_client::store::messages::ChatItem;

fn photo() -> PreparedPhoto {
    prepare_photo(&[255, 0, 0, 255], 1, 1, Purpose::Chat).unwrap()
}

fn history(f: &Fixture, peer: &str) -> Vec<ChatItem> {
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .current_chat_items(peer)
        .unwrap()
}

fn refused(f: &Fixture, peer: &str, target: &str, photo: &PreparedPhoto, expected: &str) {
    let before = snapshot(f);
    let rows = history(f, peer);
    for strict in [false, true] {
        let result = if strict {
            f.chats
                .send_chat_photo(
                    target.into(),
                    photo.full.clone(),
                    photo.thumbnail.clone(),
                    photo.width,
                    photo.height,
                )
                .map(|_| ())
        } else {
            f.chats
                .submit_chat_photo(
                    target.into(),
                    photo.full.clone(),
                    photo.thumbnail.clone(),
                    photo.width,
                    photo.height,
                )
                .map(|_| ())
        };
        assert_eq!(result.unwrap_err().to_string(), expected);
        assert_eq!(snapshot(f), before);
        assert!(history(f, peer) == rows);
    }
}

fn set_blocked(f: &Fixture, peer: &str, blocked: bool) {
    f.chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(peer, blocked)
        .unwrap();
}

#[test]
fn public_photo_admission_refusals_preserve_history_mls_and_outbox() {
    let f = Fixture::new();
    let peer = establish(&f);
    let photo = photo();
    refused(&f, &peer, &f.key.peer_id(), &photo, "ownPeer");
    refused(
        &f,
        &peer,
        &IdentityKey::generate().unwrap().peer_id(),
        &photo,
        "conversationNotOpen",
    );
    set_blocked(&f, &peer, true);
    refused(&f, &peer, &peer, &photo, "blocked");
    set_blocked(&f, &peer, false);
    {
        let session = f.chats.session.lock().unwrap();
        let mut own = session.store().profile_draft().unwrap().unwrap();
        own.devices.clear();
        session.store().save_profile_draft(&own).unwrap();
    }
    refused(&f, &peer, &peer, &photo, "unpublished");
}

#[test]
fn public_photo_validation_refusals_never_commit() {
    let f = Fixture::new();
    let peer = establish(&f);
    let mut photo = photo();
    photo.width = 2;
    refused(&f, &peer, &peer, &photo, "corrupted");
    photo.width = 1;
    photo.thumbnail = vec![1];
    refused(&f, &peer, &peer, &photo, "not a whole JPEG");
    photo.full = b"GIF89a".to_vec();
    refused(&f, &peer, &peer, &photo, "unsupported");
    photo.full = vec![0xff, 0xd8, 0];
    refused(&f, &peer, &peer, &photo, "corrupted");
}

#[test]
fn missing_network_refuses_photo_before_upload_or_commit() {
    let f = Fixture::new();
    let peer = establish(&f);
    refused(&f, &peer, &peer, &photo(), "notConnected");
}
