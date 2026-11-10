use super::*;
use crate::attachment::{self, Attachment};
use crate::photo::prepare::{prepare_photo, Purpose};
use crate::sticker_pack::{from_peer, Pack};
use crate::store::sticker_packs::uploads::{PendingUpload, UploadPlan};
use dyapp_identity::sha256;
use image_webp::{ColorType, WebPEncoder};
use prost::Message;

fn photo() -> (Attachment, Vec<u8>) {
    let prepared = prepare_photo(&[255, 0, 0, 255], 1, 1, Purpose::Chat).unwrap();
    let (mut descriptor, blob) = attachment::seal(&prepared.full).unwrap();
    descriptor.mime = "image/jpeg".into();
    descriptor.width = prepared.width;
    descriptor.height = prepared.height;
    descriptor.preview = prepared.thumbnail;
    (descriptor, blob)
}

fn pack() -> Pack {
    #[derive(Clone, PartialEq, Message)]
    struct Record {
        #[prost(bytes = "vec", tag = "1")]
        manifest: Vec<u8>,
        #[prost(bytes = "vec", repeated, tag = "2")]
        files: Vec<Vec<u8>>,
    }
    let mut image = Vec::new();
    WebPEncoder::new(&mut image)
        .encode(&[255, 0, 0, 255], 1, 1, ColorType::Rgba8)
        .unwrap();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format": 1, "name": "Cats", "stickers": [{
            "file": "cat.webp", "emoji": "😺", "sha256": dyapp_identity::hex(&sha256(&image))
        }]
    }))
    .unwrap();
    let id = sha256(&manifest);
    from_peer(
        &Record {
            manifest,
            files: vec![image],
        }
        .encode_to_vec(),
        &id,
    )
    .unwrap()
}

fn confirm_upload(f: &mut Fixture, pack: &Pack) -> PendingUpload {
    let (mut descriptor, _) = attachment::seal(pack.sticker_bytes(0).unwrap()).unwrap();
    descriptor.mime = "image/webp".into();
    descriptor.width = 1;
    descriptor.height = 1;
    let UploadPlan::Upload(pending) = f
        .session
        .store
        .plan_sticker_upload(&descriptor, crate::now())
        .unwrap()
    else {
        panic!("the refused upload must retain its reservation");
    };
    f.session
        .store
        .finish_sticker_upload(&pending, crate::now())
        .unwrap();
    pending
}

#[tokio::test]
async fn photo_upload_refusal_never_stages_a_message() {
    let mut f = open_chat(fixture());
    let (descriptor, blob) = photo();
    let sealed = f.session.store().sealed_device().unwrap();
    assert!(matches!(
        f.session.submit_attachment_to(&mut [], &f.owner, &f.peer, &descriptor, &blob, &f.seal).await,
        Err(Error::Client(crate::Error::Request(message))) if message == "no node"
    ));
    // Legacy strict group sends must also refuse an upload before advancing MLS.
    assert!(matches!(
        f.session.send_attachment(&mut [], &f.owner, &f.group, &descriptor, &blob, &f.seal).await,
        Err(Error::Client(crate::Error::Request(message))) if message == "no node"
    ));
    assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
    assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
    assert_eq!(
        f.session
            .store()
            .current_chat_attachments(&f.peer)
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn photo_post_upload_phase_retains_id_and_exact_bytes_after_commit() {
    let mut f = open_chat(fixture());
    let (descriptor, _) = photo();
    // Exercise the production phase reached after the upload completion signal, without network timing.
    let outcome = f
        .session
        .submit_uploaded_attachment(&mut [], &f.group, &descriptor, &f.seal)
        .await
        .unwrap();
    let id = queued(&outcome);
    let rows = f.session.store().current_chat_attachments(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    assert_eq!(rows[0].message.attachment, descriptor);
    let before = writes(&f.session);
    drop(f.session);
    let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
    reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
    assert_eq!(writes(&reopened), before);
    assert_eq!(
        reopened.store().current_chat_attachments(&f.peer).unwrap()[0]
            .key
            .id,
        id
    );
}

#[tokio::test]
async fn photo_closed_or_blocked_guard_runs_before_upload() {
    for blocked in [false, true] {
        let mut f = ineligible_chat(blocked);
        let (descriptor, blob) = photo();
        let sealed = f.session.store().sealed_device().unwrap();
        let error = f
            .session
            .submit_attachment_to(&mut [], &f.owner, &f.peer, &descriptor, &blob, &f.seal)
            .await
            .unwrap_err();
        if blocked {
            assert!(matches!(
                error,
                Error::Store(crate::store::Error::BlockedPeer)
            ));
        } else {
            assert!(matches!(error, Error::ConversationNotOpen));
        }
        assert_eq!(f.session.store().sealed_device().unwrap(), sealed);
        assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
    }
}

#[tokio::test]
async fn saved_pack_refusal_is_unstaged_and_confirmed_reuse_is_explicitly_queued() {
    let mut f = open_chat(fixture());
    let pack = pack();
    assert!(matches!(
        f.session.submit_pack_sticker_to(&mut [], &f.owner, &f.peer, &pack, 0, &f.seal).await,
        Err(Error::Client(crate::Error::Request(message))) if message == "no node"
    ));
    assert_eq!(
        f.session
            .store()
            .current_chat_stickers(&f.peer)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
    let pending = confirm_upload(&mut f, &pack);
    let outcome = f
        .session
        .submit_pack_sticker_to(&mut [], &f.owner, &f.peer, &pack, 0, &f.seal)
        .await
        .unwrap();
    let id = queued(&outcome);
    let rows = f.session.store().current_chat_stickers(&f.peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    let descriptor = rows[0].sticker.attachment.as_ref().unwrap();
    assert_eq!(descriptor.release_secret, [0; 32]);
    assert_eq!(descriptor.blob_sha256, pending.attachment.blob_sha256);
    assert_ne!(pending.attachment.release_secret, [0; 32]);
}

#[tokio::test]
async fn saved_pack_legacy_send_keeps_its_committed_client_failure_contract() {
    let mut f = open_chat(fixture());
    let pack = pack();
    confirm_upload(&mut f, &pack);
    let id = f
        .session
        .send_pack_sticker_to(&mut [], &f.owner, &f.peer, &pack, 0, &f.seal)
        .await
        .unwrap();
    assert_eq!(
        f.session.store().current_chat_stickers(&f.peer).unwrap()[0]
            .key
            .id,
        id
    );
}

#[tokio::test]
async fn saved_pack_owner_and_index_errors_never_report_commitment() {
    let mut f = open_chat(fixture());
    let pack = pack();
    let stranger = Identity::generate();
    assert!(matches!(
        f.session
            .submit_pack_sticker_to(&mut [], &stranger, &f.peer, &pack, 0, &f.seal)
            .await,
        Err(Error::Client(crate::Error::Reply("sticker owner mismatch")))
    ));
    assert!(matches!(
        f.session
            .submit_pack_sticker_to(&mut [], &f.owner, &f.peer, &pack, 1, &f.seal)
            .await,
        Err(Error::Client(crate::Error::Reply("unknown sticker")))
    ));
    assert_eq!(writes(&f.session), [] as [Vec<u8>; 0]);
}
