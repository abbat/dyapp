use super::*;

struct SavedPack {
    root: std::path::PathBuf,
    slot: crate::slots::Slot,
    identity: Identity,
    sessions: chats::Sessions,
    pack_id: [u8; 32],
}

fn saved_pack() -> SavedPack {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build")
        .join(format!(
            "native-sticker-send-{}",
            Identity::generate().peer_id()
        ));
    std::fs::create_dir_all(&root).unwrap();
    let (slot, identity) = crate::slots::create(&root, "file", &crate::save, &crate::load).unwrap();
    let sessions = chats::Sessions::default();
    let pack_id = super::super::import(
        &root,
        &slot.dir,
        &identity,
        &sessions,
        &super::super::native_tests::archive_count(1),
    )
    .unwrap();
    SavedPack {
        root,
        slot,
        identity,
        sessions,
        pack_id,
    }
}

#[tokio::test]
async fn invalid_saved_entry_or_closed_chat_never_opens_mls_or_stages_a_message() {
    let SavedPack {
        root,
        slot,
        identity,
        sessions,
        pack_id,
    } = saved_pack();
    let peer_id = Identity::generate().peer_id();
    let device = Identity::generate();
    let cache = Mutex::new(ProfileCache::default());
    let mut selection = Selection {
        peer_id: peer_id.clone(),
        pack_id,
        index: 1,
    };
    assert_eq!(
        send(&cache, &root, &slot.dir, &identity, &device, &selection)
            .await
            .unwrap_err(),
        "invalidIndex"
    );
    selection.index = 0;
    selection.pack_id = [0; 32];
    assert_eq!(
        send(&cache, &root, &slot.dir, &identity, &device, &selection)
            .await
            .unwrap_err(),
        "notFound"
    );
    selection.pack_id = pack_id;
    assert_eq!(
        send(&cache, &root, &slot.dir, &identity, &device, &selection)
            .await
            .unwrap_err(),
        "invalidTarget"
    );
    selection.peer_id = identity.peer_id();
    assert_eq!(
        send(&cache, &root, &slot.dir, &identity, &device, &selection)
            .await
            .unwrap_err(),
        "invalidTarget"
    );
    let store = chats::open(&slot.dir, &identity).unwrap();
    assert!(store.current_chat_stickers(&peer_id).unwrap().is_empty());
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
    assert!(store.sealed_device().unwrap().is_none());
    let (_, key) = super::super::local_store(&root, &slot.dir, &identity, &sessions).unwrap();
    assert!(store.saved_sticker_pack(&key, &pack_id).unwrap().is_some());
}
