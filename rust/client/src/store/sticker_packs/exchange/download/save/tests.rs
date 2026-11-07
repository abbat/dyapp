use super::*;
use crate::store::sticker_packs::{self, exchange::tests::database};
use dyapp_identity::Identity;

fn intent(store: &Store, peer: &str, pack: &Pack) -> PackDownloadJob {
    let job = PackDownloadJob {
        peer: peer.into(),
        pack_id: *pack.id(),
        sticker_hash: pack.stickers()[0].sha256,
        token: [3; 16],
    };
    store.db.execute(
        "INSERT INTO `sticker_pack_requests` (`peer_id`, `pack_id`, `sticker_hash`, `request_token`) VALUES (?1, ?2, ?3, ?4)",
        params![peer, job.pack_id.as_slice(), job.sticker_hash.as_slice(), job.token.as_slice()]).unwrap();
    job
}
fn pending(store: &Store, job: &PackDownloadJob) -> bool {
    store
        .pending_sticker_pack(&job.peer, &job.pack_id)
        .unwrap()
        .is_some()
}
fn no_file(store: &Store, job: &PackDownloadJob) {
    assert!(store
        .saved_sticker_pack(&[7; 32], &job.pack_id)
        .unwrap()
        .is_none());
}

#[test]
fn current_requested_pack_publication_and_existing_file_retire_intent() {
    let (mut store, owner, path, peer) = database();
    let pack = sticker_packs::tests::pack("Requested");
    let job = intent(&store, &peer, &pack);
    assert!(store
        .save_requested_sticker_pack(&[7; 32], &[9], &job, &pack)
        .unwrap());
    assert!(!pending(&store, &job));
    assert!(store
        .save_requested_sticker_pack(&[7; 32], &[9], &job, &pack)
        .is_err());
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert_eq!(
        store
            .saved_sticker_pack(&[7; 32], pack.id())
            .unwrap()
            .unwrap()
            .encode(),
        pack.encode()
    );
    let job = intent(&store, &peer, &pack);
    assert!(!store
        .save_requested_sticker_pack(&[7; 32], &[9], &job, &pack)
        .unwrap());
    assert!(!pending(&store, &job));
}

#[test]
fn stale_wrong_unsolicited_or_disallowed_request_never_publishes() {
    let (mut store, _, _, peer) = database();
    let pack = sticker_packs::tests::pack("Requested");
    let job = intent(&store, &peer, &pack);
    let mut stale = job.clone();
    stale.token = [4; 16];
    assert!(store
        .save_requested_sticker_pack(&[7; 32], &[9], &stale, &pack)
        .is_err());
    stale = job.clone();
    stale.peer = Identity::generate().peer_id();
    assert!(store
        .save_requested_sticker_pack(&[7; 32], &[9], &stale, &pack)
        .is_err());
    assert!(store
        .save_requested_sticker_pack(&[7; 32], &[10], &job, &pack)
        .is_err());
    let wrong = sticker_packs::tests::pack("Other manifest");
    assert!(store
        .save_requested_sticker_pack(&[7; 32], &[9], &job, &wrong)
        .is_err());
    assert!(store
        .saved_sticker_pack(&[7; 32], wrong.id())
        .unwrap()
        .is_none());
    store.set_blocked(&peer, true).unwrap();
    assert!(matches!(
        store.save_requested_sticker_pack(&[7; 32], &[9], &job, &pack),
        Err(Error::Request(crate::store::Error::BlockedPeer))
    ));
    store.set_blocked(&peer, false).unwrap();
    store
        .db
        .execute(
            "UPDATE `conversations` SET `state` = 4 WHERE `peer_id` = ?",
            [&peer],
        )
        .unwrap();
    assert!(matches!(
        store.save_requested_sticker_pack(&[7; 32], &[9], &job, &pack),
        Err(Error::Request(crate::store::Error::ChatClosed))
    ));
    assert!(pending(&store, &job));
    no_file(&store, &job);
}

#[test]
fn clicked_hash_and_quota_guards_preserve_request_without_publication() {
    let (mut store, _, _, peer) = database();
    let pack = sticker_packs::tests::pack("Requested");
    let job = intent(&store, &peer, &pack);
    store
        .db
        .execute(
            "UPDATE `sticker_pack_requests` SET `sticker_hash` = ?",
            [[99; 32].as_slice()],
        )
        .unwrap();
    let wrong = PackDownloadJob {
        sticker_hash: [99; 32],
        ..job.clone()
    };
    assert!(store
        .save_requested_sticker_pack(&[7; 32], &[9], &wrong, &pack)
        .is_err());
    assert!(pending(&store, &job));
    no_file(&store, &job);
    store
        .db
        .execute(
            "UPDATE `sticker_pack_requests` SET `sticker_hash` = ?",
            [job.sticker_hash.as_slice()],
        )
        .unwrap();
    assert!(matches!(
        store.save_requested_pack_with_limit(&[7; 32], &[9], &job, &pack, 0),
        Err(Error::Quota)
    ));
    assert!(pending(&store, &job));
    no_file(&store, &job);
}

#[test]
fn failed_sqlite_commit_preserves_intent_and_published_file_recovers_idempotently() {
    let (mut store, owner, path, peer) = database();
    let pack = sticker_packs::tests::pack("Requested");
    let job = intent(&store, &peer, &pack);
    // Deferred constraint fails at commit, after the file rename, rather than in the guard.
    store.db.execute_batch(
        "CREATE TABLE `test_parent` (`id` INTEGER PRIMARY KEY);
         CREATE TABLE `test_child` (`id` INTEGER REFERENCES `test_parent`(`id`) DEFERRABLE INITIALLY DEFERRED);
         CREATE TABLE `test_gate` (`enabled` INTEGER);
         INSERT INTO `test_gate` VALUES (1);
         CREATE TRIGGER `test_commit_failure` AFTER DELETE ON `sticker_pack_requests`
         WHEN (SELECT `enabled` FROM `test_gate`) = 1
         BEGIN INSERT INTO `test_child` VALUES (42); END;").unwrap();
    assert!(matches!(
        store.save_requested_sticker_pack(&[7; 32], &[9], &job, &pack),
        Err(Error::Sql(_))
    ));
    assert!(pending(&store, &job));
    assert!(store
        .saved_sticker_pack(&[7; 32], pack.id())
        .unwrap()
        .is_some());
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(pending(&store, &job));
    store
        .db
        .execute("UPDATE `test_gate` SET `enabled` = 0", [])
        .unwrap();
    assert!(!store
        .save_requested_sticker_pack(&[7; 32], &[9], &job, &pack)
        .unwrap());
    assert!(!pending(&store, &job));
}
