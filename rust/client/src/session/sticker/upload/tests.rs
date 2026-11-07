use super::*;
use crate::session::tests::{bob_invited, Invite};
use crate::store::InboxKey;
use image_webp::{ColorType, WebPEncoder};
use prost::Message;

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
    let hash = sha256(&image)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format":1,"name":"Cats","stickers":[{"file":"cat.webp","emoji":"😺","sha256":hash}]
    }))
    .unwrap();
    let id = sha256(&manifest);
    crate::sticker_pack::from_peer(
        &Record {
            manifest,
            files: vec![image],
        }
        .encode_to_vec(),
        &id,
    )
    .unwrap()
}

#[test]
fn reconstruction_binds_both_hashes_and_metadata_comes_from_validated_pack() {
    let pack = pack();
    let (sticker, candidate) = prepare(&pack, 0).unwrap();
    assert_eq!(sticker.pack_id, *pack.id());
    assert_eq!(sticker.sha256, pack.stickers()[0].sha256);
    assert_eq!(
        (sticker.pack_name.as_str(), sticker.emoji.as_str()),
        ("Cats", "😺")
    );
    assert!(sticker.attachment.is_none());
    assert!(candidate.release_secret != [0; 32]);
    assert_eq!((candidate.width, candidate.height), (1, 1));
    let bytes = pack.sticker_bytes(0).unwrap();
    let blob = ciphertext(&candidate, bytes).unwrap();
    assert_eq!(attachment::open(&candidate, &blob).unwrap(), bytes);
    assert!(ciphertext(&candidate, b"wrong").is_err());
    let mut corrupt = candidate.clone();
    corrupt.key[0] ^= 1;
    assert!(ciphertext(&corrupt, bytes).is_err());
    corrupt = candidate.clone();
    corrupt.blob_sha256[0] ^= 1;
    assert!(ciphertext(&corrupt, bytes).is_err());
    assert!(prepare(&pack, 1).is_err());
}

#[tokio::test]
async fn failed_upload_does_not_stage_but_confirmed_reuse_strips_owner_capability() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/sticker-session-tests");
    std::fs::create_dir_all(&root).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = root.join(format!("{:032x}.db", u128::from_be_bytes(random)));
    let seal = [5; 32];
    let Invite {
        mut session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&path, &seal);
    let key = |id| InboxKey {
        mailbox: [9; 32],
        id: [id; 16],
    };
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal)
        .unwrap();
    let peer = alice_id.peer_id();
    let pack = pack();
    assert!(matches!(
        session
            .send_pack_sticker_to(&mut [], &bob_id, &peer, &pack, 0, &seal)
            .await,
        Err(Error::ConversationNotOpen)
    ));
    let request = crate::chat::message(
        &mut group,
        &alice,
        &crate::chat::Body::text_with_id([1; 16], b"hello".to_vec()),
    )
    .unwrap();
    session
        .stage_envelope(&key(2), &request[0].mls, &profiles, &seal)
        .unwrap();
    session.stage_text(group.id(), b"reply", &seal).unwrap();
    let before = session.store.due_outbound(i64::MAX).unwrap().len();
    assert!(session
        .send_pack_sticker_to(&mut [], &alice_id, &peer, &pack, 0, &seal)
        .await
        .is_err());
    session.store.set_blocked(&peer, true).unwrap();
    assert!(matches!(
        session
            .send_pack_sticker_to(&mut [], &bob_id, &peer, &pack, 0, &seal)
            .await,
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
    session.store.set_blocked(&peer, false).unwrap();
    assert!(session
        .send_pack_sticker_to(&mut [], &bob_id, &peer, &pack, 0, &seal)
        .await
        .is_err());
    assert!(session
        .store
        .current_chat_stickers(&peer)
        .unwrap()
        .is_empty());
    assert_eq!(session.store.due_outbound(i64::MAX).unwrap().len(), before);
    let (_, candidate) = prepare(&pack, 0).unwrap();
    let pending = match session
        .store
        .plan_sticker_upload(&candidate, crate::now())
        .unwrap()
    {
        UploadPlan::Upload(value) => value,
        _ => panic!("failed upload must remain pending"),
    };
    session
        .store
        .finish_sticker_upload(&pending, crate::now())
        .unwrap();
    // No node is needed to reuse metadata; mailbox delivery may remain pending.
    let id = session
        .send_pack_sticker_to(&mut [], &bob_id, &peer, &pack, 0, &seal)
        .await
        .unwrap();
    let rows = session.store.current_chat_stickers(&peer).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.id, id);
    let descriptor = rows[0].sticker.attachment.as_ref().unwrap();
    assert_eq!(descriptor.release_secret, [0; 32]);
    assert_eq!(descriptor.blob_sha256, pending.attachment.blob_sha256);
    assert_eq!(descriptor.key, pending.attachment.key);
    assert!(session.store.due_outbound(i64::MAX).unwrap().len() > before);
}
