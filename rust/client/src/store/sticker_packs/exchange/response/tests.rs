use super::*;
use crate::chat::sticker::Sticker;
use crate::store::sticker_packs::exchange::tests::{database, selected};

// Literal times around the rolling 24-hour boundary after the first response at 100 ms.
const DUE: u64 = 86_400_100;
const EARLY: u64 = 86_400_099;

fn inbox(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [9; 32],
        id: [id; 16],
    }
}
fn sender(peer: &str) -> VerifiedChatSender<'_> {
    VerifiedChatSender {
        group: &[9],
        author: peer,
        device: &[8; 32],
    }
}
fn authorize(store: &mut Store, owner: &dyapp_identity::Identity, pack: u8) {
    let own = owner.peer_id();
    store
        .record_chat_sticker(
            sender(&own),
            &Sticker {
                id: [pack; 16],
                hlc: pack.into(),
                pack_id: [pack; 32],
                sha256: [7; 32],
                emoji: "😺".into(),
                pack_name: "Cats".into(),
                attachment: None,
            },
            false,
        )
        .unwrap();
}

#[test]
fn only_shared_packs_queue_and_duplicate_requests_keep_generation_after_restart() {
    let (mut store, owner, path, peer) = database();
    selected(&mut store, &peer, 1); // A received sticker cannot grant sharing permission.
    let request = PackRequest { pack_id: [1; 32] };
    assert!(!store
        .stage_sticker_pack_request_inbound(b"drop", &inbox(1), sender(&peer), &request, 100)
        .unwrap());
    assert!(store.pending_acks().unwrap().contains(&inbox(1)));
    assert!(store.due_sticker_pack_responses(100).unwrap().is_empty());
    authorize(&mut store, &owner, 1);
    assert!(store
        .stage_sticker_pack_request_inbound(b"queued", &inbox(2), sender(&peer), &request, 100)
        .unwrap());
    let jobs = store.due_sticker_pack_responses(100).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        (&jobs[0].peer, &jobs[0].group, jobs[0].pack_id),
        (&peer, &vec![9], [1; 32])
    );
    assert!(!store
        .stage_sticker_pack_request_inbound(b"duplicate", &inbox(3), sender(&peer), &request, 200)
        .unwrap());
    assert!(!store
        .stage_sticker_pack_request_inbound(b"replay", &inbox(2), sender(&peer), &request, 300)
        .unwrap());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"duplicate");
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert!(store.due_sticker_pack_responses(300).unwrap() == jobs);
    assert!(store.due_sticker_pack_responses(u64::MAX).is_err());
}

#[test]
fn retry_deadlines_match_outbox_backoff_and_duplicates_cannot_reset_them() {
    let (mut store, owner, _, peer) = database();
    authorize(&mut store, &owner, 1);
    let request = PackRequest { pack_id: [1; 32] };
    store
        .stage_sticker_pack_request_inbound(b"queued", &inbox(1), sender(&peer), &request, 0)
        .unwrap();
    let job = store.due_sticker_pack_responses(0).unwrap().remove(0);
    let mut stale = job.clone();
    stale.token[0] ^= 1;
    assert!(!store.defer_sticker_pack_response(&stale, 100).unwrap());
    let mut now = 100;
    for delay in [
        1000, 2000, 4000, 8000, 16000, 32000, 64000, 128_000, 256_000, 512_000, 1_024_000,
        2_048_000, 3_600_000, 3_600_000,
    ] {
        assert!(store.defer_sticker_pack_response(&job, now).unwrap());
        let deadline = now + delay;
        if now == 100 {
            assert!(!store
                .stage_sticker_pack_request_inbound(
                    b"duplicate",
                    &inbox(2),
                    sender(&peer),
                    &request,
                    500
                )
                .unwrap());
        }
        assert!(store
            .due_sticker_pack_responses(deadline - 1)
            .unwrap()
            .is_empty());
        assert!(store.due_sticker_pack_responses(deadline).unwrap() == vec![job.clone()]);
        now = deadline;
    }
    assert!(store.defer_sticker_pack_response(&job, u64::MAX).is_err());
}

#[test]
fn queue_cap_batch_size_and_rolling_cooldown_boundaries_are_literal() {
    let (mut store, owner, _, peer) = database();
    for id in 1..=64 {
        authorize(&mut store, &owner, id);
        assert!(store
            .stage_sticker_pack_request_inbound(
                b"queued",
                &inbox(id),
                sender(&peer),
                &PackRequest { pack_id: [id; 32] },
                0
            )
            .unwrap());
    }
    let due = store.due_sticker_pack_responses(0).unwrap();
    assert_eq!(due.len(), 32);
    let first = due[0].clone();
    authorize(&mut store, &owner, 65);
    assert!(!store
        .stage_sticker_pack_request_inbound(
            b"cap",
            &inbox(65),
            sender(&peer),
            &PackRequest { pack_id: [65; 32] },
            0
        )
        .unwrap());
    assert!(store.pending_acks().unwrap().contains(&inbox(65)));
    // Seed a completed response timestamp; real outbox completion is tested separately.
    store
        .db
        .execute(
            "UPDATE `sticker_pack_responses` SET `last_answered_ms` = 100,
         `pending_group` = NULL, `job_token` = NULL WHERE `pack_id` = ?",
            [first.pack_id.as_slice()],
        )
        .unwrap();
    let request = PackRequest {
        pack_id: first.pack_id,
    };
    assert!(!store
        .stage_sticker_pack_request_inbound(b"rollback", &inbox(66), sender(&peer), &request, 99)
        .unwrap());
    assert!(!store
        .stage_sticker_pack_request_inbound(b"early", &inbox(67), sender(&peer), &request, EARLY)
        .unwrap());
    assert!(store
        .stage_sticker_pack_request_inbound(b"boundary", &inbox(68), sender(&peer), &request, DUE)
        .unwrap());
    assert!(!store.defer_sticker_pack_response(&first, DUE).unwrap());
    let count: i64 = store
        .db
        .query_row(
            "SELECT COUNT(*) FROM `sticker_pack_responses` WHERE `pending_group` IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 64);
}
