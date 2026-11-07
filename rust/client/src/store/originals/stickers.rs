//! Exercise actual shared originals files, not a separate sticker cache.
use super::*;
use crate::{
    chat::{sticker::Sticker, AttachmentMessage},
    sticker_pack::Pack,
    store::{messages::VerifiedChatSender, tests::catalog_group},
};
use dyapp_identity::Identity;

fn fixture() -> (Store, Identity, PathBuf, String, Pack, Sticker, Vec<u8>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/sticker-originals-tests");
    fs::create_dir_all(&root).unwrap();
    let owner = Identity::generate();
    let dir = root.join(owner.peer_id());
    fs::create_dir(&dir).unwrap();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&dir.join("client.db"), &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    catalog_group(&mut store, &[10], &peer);
    let pack = crate::store::sticker_packs::tests::pack("Retained pack");
    store.save_sticker_pack(&[7; 32], &pack).unwrap();
    let bytes = pack.sticker_bytes(0).unwrap().to_vec();
    let (mut descriptor, _) = attachment::seal(&bytes).unwrap();
    descriptor.mime = "image/webp".into();
    descriptor.width = 1;
    descriptor.height = 1;
    descriptor.release_secret = [0; 32];
    let sticker = Sticker {
        id: [1; 16],
        hlc: 1,
        pack_id: *pack.id(),
        sha256: pack.stickers()[0].sha256,
        emoji: pack.stickers()[0].emoji.clone(),
        pack_name: pack.name().into(),
        attachment: Some(descriptor),
    };
    (store, owner, dir, peer, pack, sticker, bytes)
}
fn sender<'a>(peer: &'a str, group: &'a [u8]) -> VerifiedChatSender<'a> {
    VerifiedChatSender {
        group,
        author: peer,
        device: &[8; 32],
    }
}

#[test]
fn sealed_sticker_original_survives_restart_and_follows_last_history_reference() {
    let (mut store, owner, dir, peer, pack, sticker, bytes) = fixture();
    store
        .record_chat_sticker(sender(&peer, &[9]), &sticker, true)
        .unwrap();
    store
        .record_chat_sticker(sender(&peer, &[10]), &sticker, true)
        .unwrap();
    let descriptor = sticker.download().unwrap();
    keep(&dir, &[7; 32], descriptor, &bytes).unwrap();
    store.sweep_originals(&dir).unwrap();
    let frame = sticker
        .first_frame(&read(&dir, &[7; 32], descriptor).unwrap())
        .unwrap();
    assert_eq!((frame.width, frame.height), (1, 1));
    assert_eq!(frame.pixels, [1, 2, 3, 255]);
    drop(store);
    let mut store = Store::open(&dir.join("client.db"), &owner).unwrap();
    assert_eq!(read(&dir, &[7; 32], descriptor).unwrap(), bytes);
    store.clear_chat(&[9]).unwrap();
    store.sweep_originals(&dir).unwrap();
    assert!(is_kept(&dir, &descriptor.blob_sha256));
    store.clear_chat(&[10]).unwrap();
    store.sweep_originals(&dir).unwrap();
    assert!(!is_kept(&dir, &descriptor.blob_sha256));
    assert!(store
        .saved_sticker_pack(&[7; 32], pack.id())
        .unwrap()
        .is_some());
}

#[test]
fn stickers_share_photo_lru_and_off_setting_but_saved_packs_are_never_evicted() {
    let (mut store, _, dir, peer, pack, sticker, bytes) = fixture();
    store
        .record_chat_sticker(sender(&peer, &[9]), &sticker, true)
        .unwrap();
    let descriptor = sticker.download().unwrap();
    keep(&dir, &[7; 32], descriptor, &bytes).unwrap();
    let mut photos = Vec::new();
    for id in 2..=3 {
        let plain = vec![id; 600 * 1024];
        let (photo, _) = attachment::seal(&plain).unwrap();
        store
            .record_chat_attachment(
                sender(&peer, &[9]),
                &Body::Attachment(AttachmentMessage {
                    id: [id; 16],
                    hlc: id.into(),
                    attachment: photo.clone(),
                }),
                true,
            )
            .unwrap();
        keep(&dir, &[7; 32], &photo, &plain).unwrap();
        photos.push(photo);
    }
    // Explicit fixture mtimes make recency order independent of clock resolution and sleeps.
    for (index, photo) in photos.iter().chain(std::iter::once(descriptor)).enumerate() {
        File::options()
            .write(true)
            .open(path(&dir, &photo.blob_sha256))
            .unwrap()
            .set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(100 + index as u64),
            )
            .unwrap();
    }
    store
        .set_media_cache_config(MediaCacheConfig {
            enabled: true,
            limit_bytes: 1024 * 1024,
        })
        .unwrap();
    store.sweep_originals(&dir).unwrap();
    assert!(!is_kept(&dir, &photos[0].blob_sha256));
    assert!(is_kept(&dir, &photos[1].blob_sha256));
    assert!(is_kept(&dir, &descriptor.blob_sha256));
    store
        .set_media_cache_config(MediaCacheConfig {
            enabled: false,
            limit_bytes: 1024 * 1024,
        })
        .unwrap();
    store.sweep_originals(&dir).unwrap();
    assert!(!is_kept(&dir, &photos[1].blob_sha256));
    assert!(!is_kept(&dir, &descriptor.blob_sha256));
    assert_eq!(store.current_chat_stickers(&peer).unwrap().len(), 1);
    assert!(store
        .saved_sticker_pack(&[7; 32], pack.id())
        .unwrap()
        .is_some());
}
