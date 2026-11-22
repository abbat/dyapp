use super::*;
use crate::chat::{pack_control::PackRequest, sticker::Sticker};
use crate::store::{messages::VerifiedChatSender, InboxKey};
use dyapp_identity::Identity;

// Literal times around the rolling 24-hour boundary after the first response at 100 ms.
const DUE: u64 = 86_400_100;
const EARLY: u64 = 86_400_099;

fn fixture() -> (
    dyapp_test_support::TestDir,
    Store,
    Identity,
    PackResponseJob,
    PackAnswer,
) {
    let (test_dir, mut store, owner, _, peer) =
        crate::store::sticker_packs::exchange::tests::database();
    let pack = sticker_packs::tests::pack("Cats");
    store.save_sticker_pack(&[5; 32], &pack).unwrap();
    let own = owner.peer_id();
    store
        .record_chat_sticker(
            VerifiedChatSender {
                group: &[9],
                author: &own,
                device: &[8; 32],
            },
            &Sticker {
                id: [1; 16],
                hlc: 1,
                pack_id: *pack.id(),
                sha256: pack.stickers()[0].sha256,
                emoji: "😺".into(),
                pack_name: "Cats".into(),
                attachment: None,
            },
            false,
        )
        .unwrap();
    store
        .stage_sticker_pack_request_inbound(
            b"queued",
            &InboxKey {
                mailbox: [9; 32],
                id: [1; 16],
            },
            VerifiedChatSender {
                group: &[9],
                author: &peer,
                device: &[8; 32],
            },
            &PackRequest {
                pack_id: *pack.id(),
            },
            0,
        )
        .unwrap();
    let job = store.due_sticker_pack_responses(0).unwrap().remove(0);
    (test_dir, store, owner, job, answer(&pack))
}
fn answer(pack: &crate::sticker_pack::Pack) -> PackAnswer {
    let (mut attachment, _) = crate::attachment::seal(&pack.encode()).unwrap();
    attachment.release_secret = [0; 32];
    attachment.mime = "application/octet-stream".into();
    PackAnswer {
        pack_id: *pack.id(),
        attachment,
    }
}
fn request_at(store: &mut Store, job: &PackResponseJob, id: u8, sealed: &[u8], now: u64) -> bool {
    let sender = VerifiedChatSender {
        group: &job.group,
        author: &job.peer,
        device: &[8; 32],
    };
    let request = PackRequest {
        pack_id: job.pack_id,
    };
    store
        .stage_sticker_pack_request_inbound(
            sealed,
            &InboxKey {
                mailbox: [9; 32],
                id: [id; 16],
            },
            sender,
            &request,
            now,
        )
        .unwrap()
}
fn last(store: &Store) -> Option<i64> {
    store
        .db
        .query_row(
            "SELECT `last_answered_ms` FROM `sticker_pack_responses`",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn durable_response_and_cooldown_survive_restart_and_reject_stale_workers() {
    let (_test_dir, mut store, owner, job, answer) = fixture();
    let path = std::path::PathBuf::from(store.db.path().unwrap());
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    store
        .stage_sticker_pack_response(b"sent", std::slice::from_ref(&write), &job, &answer, 100)
        .unwrap();
    assert_eq!(last(&store), Some(100));
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"sent");
    assert_eq!(
        store.due_outbound(i64::MAX).unwrap()[0].as_bytes(),
        write.as_bytes()
    );
    assert!(store.due_sticker_pack_responses(100).unwrap().is_empty());
    assert!(!request_at(&mut store, &job, 2, b"early", EARLY));
    assert!(request_at(&mut store, &job, 3, b"boundary", DUE));
    assert!(!store.retire_sticker_pack_response(&job).unwrap());
    assert!(!store.defer_sticker_pack_response(&job, DUE).unwrap());
    assert!(store
        .stage_sticker_pack_response(b"stale", &[write], &job, &answer, DUE)
        .is_err());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"boundary");
    assert_eq!(store.due_sticker_pack_responses(DUE).unwrap().len(), 1);
    assert_eq!(last(&store), Some(100));
}

#[test]
fn response_outbox_failure_or_removed_pack_never_advances_cooldown() {
    let (_test_dir, mut store, _, job, answer) = fixture();
    let write = crate::prepare_put(&[1; 32], vec![9]).unwrap();
    assert!(store
        .stage_sticker_pack_response(b"empty", &[], &job, &answer, 100)
        .is_err());
    assert!(store
        .stage_sticker_pack_response(b"", std::slice::from_ref(&write), &job, &answer, 100)
        .is_err());
    assert_eq!(last(&store), None);
    store
        .stage_outbound(b"before", std::slice::from_ref(&write))
        .unwrap();
    store
        .db
        .execute(
            "UPDATE `outbox` SET `signed_envelope` = ?",
            [b"corrupt".as_slice()],
        )
        .unwrap();
    assert!(store
        .stage_sticker_pack_response(b"after", std::slice::from_ref(&write), &job, &answer, 100)
        .is_err());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"before");
    assert_eq!(last(&store), None);
    store.remove_saved_sticker_pack(&job.pack_id).unwrap();
    assert!(matches!(
        store.stage_sticker_pack_response(b"removed", &[write], &job, &answer, 100),
        Err(Error::StickerPackUnavailable)
    ));
    assert_eq!(store.due_sticker_pack_responses(100).unwrap().len(), 1);
    assert!(store.retire_sticker_pack_response(&job).unwrap());
    assert!(store.due_sticker_pack_responses(100).unwrap().is_empty());
    assert_eq!(last(&store), None);
}

#[test]
fn concurrent_response_completions_commit_only_one_outbox() {
    let (_test_dir, store, owner, job, answer) = fixture();
    let path = std::path::PathBuf::from(store.db.path().unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut tasks = Vec::new();
    for byte in [1, 2] {
        let mut connection = Store::open(&path, &owner).unwrap();
        let (job, answer, barrier) = (job.clone(), answer.clone(), barrier.clone());
        tasks.push(std::thread::spawn(move || {
            let write = crate::prepare_put(&[1; 32], vec![byte]).unwrap();
            barrier.wait();
            match connection.stage_sticker_pack_response(b"winner", &[write], &job, &answer, 100) {
                Ok(()) => true,
                Err(Error::InboxConflict) => false,
                Err(error) => panic!("unexpected completion failure: {error}"),
            }
        }));
    }
    barrier.wait();
    assert_eq!(
        tasks
            .into_iter()
            .map(|task| usize::from(task.join().unwrap()))
            .sum::<usize>(),
        1
    );
    assert_eq!(store.due_outbound(i64::MAX).unwrap().len(), 1);
    assert_eq!(last(&store), Some(100));
}
