use super::*;

#[test]
fn only_own_verified_send_grants_exact_peer_pack_and_survives_clear_restart() {
    let path = temp_db_path();
    let owner = Identity::generate();
    let own = owner.peer_id();
    let peer = Identity::generate().peer_id();
    let stranger = Identity::generate().peer_id();
    let mut store = Store::open(&path, &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    let received = sticker(1, false);
    store
        .record_chat_sticker(sender(&peer), &received, true)
        .unwrap();
    assert!(!store.shared_sticker_pack(&peer, &received.pack_id).unwrap());
    let mut shared = sticker(2, false);
    shared.pack_id = [4; 32];
    store
        .record_chat_sticker(sender(&own), &shared, false)
        .unwrap();
    assert!(store.shared_sticker_pack(&peer, &shared.pack_id).unwrap());
    assert!(!store
        .shared_sticker_pack(&stranger, &shared.pack_id)
        .unwrap());
    assert!(!store.shared_sticker_pack(&peer, &[5; 32]).unwrap());
    assert!(!store
        .record_chat_sticker(sender(&own), &shared, false)
        .unwrap());
    let mut conflict = shared.clone();
    conflict.pack_id = [5; 32];
    assert!(store
        .record_chat_sticker(sender(&own), &conflict, false)
        .is_err());
    assert!(!store.shared_sticker_pack(&peer, &[5; 32]).unwrap());
    assert!(store.shared_sticker_pack(&own, &shared.pack_id).is_err());
    assert!(store.shared_sticker_pack("", &shared.pack_id).is_err());
    store.clear_chat_pair(&peer).unwrap();
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(store.current_chat_stickers(&peer).unwrap().is_empty());
    assert!(store.shared_sticker_pack(&peer, &shared.pack_id).unwrap());
    catalog_group(&mut store, &[10], &peer);
    assert!(store.shared_sticker_pack(&peer, &shared.pack_id).unwrap());
    let unverified = VerifiedChatSender {
        group: &[11],
        author: &own,
        device: &[8; 32],
    };
    assert!(store
        .record_chat_sticker(unverified, &conflict, false)
        .is_err());
    assert!(!store.shared_sticker_pack(&peer, &conflict.pack_id).unwrap());
}

#[test]
fn own_device_group_does_not_grant_pack_sharing() {
    let owner = Identity::generate();
    let own = owner.peer_id();
    let mut store = Store::open(&temp_db_path(), &owner).unwrap();
    catalog_group(&mut store, &[9], &own);
    store
        .record_chat_sticker(sender(&own), &sticker(1, false), false)
        .unwrap();
    let count: i64 = store
        .db
        .query_row("SELECT COUNT(*) FROM `sticker_shared_packs`", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn failed_sharing_commit_rolls_back_sticker_projection() {
    let owner = Identity::generate();
    let own = owner.peer_id();
    let peer = Identity::generate().peer_id();
    let mut store = Store::open(&temp_db_path(), &owner).unwrap();
    catalog_group(&mut store, &[9], &peer);
    store
        .db
        .execute_batch(
            "CREATE TRIGGER `reject_sharing` BEFORE INSERT ON `sticker_shared_packs`
         BEGIN SELECT RAISE(ABORT, 'fixture sharing failure'); END;",
        )
        .unwrap();
    let shared = sticker(1, false);
    assert!(store
        .record_chat_sticker(sender(&own), &shared, false)
        .is_err());
    assert!(store.current_chat_stickers(&peer).unwrap().is_empty());
    assert!(!store.shared_sticker_pack(&peer, &shared.pack_id).unwrap());
}
