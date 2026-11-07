use super::*;
use crate::chat::{pack_control::PackRequest, sticker::Sticker};
use crate::session::tests::{bob_invited, Invite};
use crate::sticker_pack::Pack;
use crate::store::{sticker_packs, InboxKey};
const SEAL: [u8; 32] = [5; 32];

fn deliver(pair: &mut Invite, body: &Body, id: u8) {
    let writes = chat::message(&mut pair.group, &pair.alice, body).unwrap();
    pair.session
        .stage_envelope(
            &InboxKey {
                mailbox: [9; 32],
                id: [id; 16],
            },
            &writes[0].mls,
            &pair.profiles,
            &SEAL,
        )
        .unwrap();
}
fn pair() -> (Invite, std::path::PathBuf) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/pack-session-tests");
    std::fs::create_dir_all(&root).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let dir = root.join(format!("{:032x}", u128::from_be_bytes(random)));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("client.db");
    let mut pair = bob_invited(&path, &SEAL);
    pair.session
        .stage_envelope(
            &InboxKey {
                mailbox: [9; 32],
                id: [1; 16],
            },
            &pair.welcome,
            &pair.profiles,
            &SEAL,
        )
        .unwrap();
    deliver(
        &mut pair,
        &Body::text_with_id([1; 16], b"hello".to_vec()),
        2,
    );
    pair.session
        .stage_text(pair.group.id(), b"reply", &SEAL)
        .unwrap();
    (pair, path)
}
fn sticker(pack: &Pack) -> Sticker {
    Sticker {
        id: [3; 16],
        hlc: 1000,
        pack_id: *pack.id(),
        sha256: pack.stickers()[0].sha256,
        emoji: pack.stickers()[0].emoji.clone(),
        pack_name: pack.name().into(),
        attachment: None,
    }
}
fn selected(pair: &mut Invite, pack: &Pack) -> MessageKey {
    deliver(pair, &Body::Sticker(Box::new(sticker(pack))), 3);
    MessageKey {
        group: pair.group.id().to_vec(),
        author: pair.alice_id.peer_id(),
        id: [3; 16],
    }
}

#[test]
fn request_staging_retains_generation_and_exact_writes_after_session_restart() {
    let (mut pair, path) = pair();
    let pack = sticker_packs::tests::pack("Cats");
    let selected = selected(&mut pair, &pack);
    let peer = pair.alice_id.peer_id();
    let (pending, writes) = pair
        .session
        .stage_sticker_pack_request(&peer, &selected, &SEAL)
        .unwrap();
    assert_eq!(pending.sticker_hash, pack.stickers()[0].sha256);
    assert!(!writes.is_empty());
    drop(pair.session);
    let mut restored = Session::open(Store::open(&path, &pair.bob_id).unwrap(), &SEAL).unwrap();
    let due = restored.due_outbound(i64::MAX).unwrap();
    assert!(writes
        .iter()
        .all(|write| due.iter().any(|item| item.as_bytes() == write.as_bytes())));
    let (again, retried) = restored
        .stage_sticker_pack_request(&peer, &selected, &SEAL)
        .unwrap();
    assert!(pending == again && !retried.is_empty());
}

#[test]
fn failed_local_request_commit_invalidates_advanced_mls_and_reopens_old_snapshot() {
    let (mut pair, path) = pair();
    let pack = sticker_packs::tests::pack("Cats");
    let selected = selected(&mut pair, &pack);
    let peer = pair.alice_id.peer_id();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER `reject_request` BEFORE INSERT ON `sticker_pack_requests`
        BEGIN SELECT RAISE(ABORT, 'fixture request failure'); END;",
    )
    .unwrap();
    let sealed = pair.session.store.sealed_device().unwrap();
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    assert!(pair
        .session
        .stage_sticker_pack_request(&peer, &selected, &SEAL)
        .is_err());
    assert!(pair.session.device.is_none() && pair.session.groups.is_empty());
    assert!(pair.session.store.sealed_device().unwrap() == sealed);
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
    assert!(pair
        .session
        .store
        .pending_sticker_pack(&peer, pack.id())
        .unwrap()
        .is_none());
    drop(pair.session);
    let mut restored = Session::open(Store::open(&path, &pair.bob_id).unwrap(), &SEAL).unwrap();
    restored
        .stage_text(pair.group.id(), b"recovered", &SEAL)
        .unwrap();
}

#[test]
fn response_staging_commits_current_job_and_stale_completion_cannot_advance_mls() {
    let (mut pair, _) = pair();
    let pack = sticker_packs::tests::pack("Cats");
    pair.session.store.save_sticker_pack(&SEAL, &pack).unwrap();
    pair.session
        .stage_sticker(pair.group.id(), &sticker(&pack), &SEAL)
        .unwrap();
    let request = Body::PackRequest(PackRequest {
        pack_id: *pack.id(),
    });
    deliver(&mut pair, &request, 3);
    let job = pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .remove(0);
    let (mut attachment, _) = crate::attachment::seal(&pack.encode()).unwrap();
    attachment.release_secret = [0; 32];
    let answer = PackAnswer {
        pack_id: *pack.id(),
        attachment,
    };
    let writes = pair
        .session
        .stage_sticker_pack_response(&job, &answer, &SEAL)
        .unwrap();
    assert!(!writes.is_empty());
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .is_empty());
    deliver(&mut pair, &request, 4);
    assert!(pair
        .session
        .store
        .due_sticker_pack_responses(crate::now() * 1000)
        .unwrap()
        .is_empty());
    let sealed = pair.session.store.sealed_device().unwrap();
    let count = pair.session.due_outbound(i64::MAX).unwrap().len();
    assert!(pair
        .session
        .stage_sticker_pack_response(&job, &answer, &SEAL)
        .is_err());
    assert!(pair.session.device.is_none());
    assert!(pair.session.store.sealed_device().unwrap() == sealed);
    assert_eq!(pair.session.due_outbound(i64::MAX).unwrap().len(), count);
}
