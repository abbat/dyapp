use super::*;

fn count(store: &Store) -> i64 {
    store
        .db
        .query_row("SELECT COUNT(*) FROM `sticker_pack_requests`", [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn hostile_or_stale_selection_and_changed_chat_state_create_no_intent() {
    let (mut store, owner, _, peer) = database();
    let key = selected(&mut store, &peer, 1);
    let request = store.sticker_pack_request(&[9], &key).unwrap();
    assert!(store.sticker_pack_request(&[10], &key).is_err());
    let mut absent = key.clone();
    absent.id = [99; 16];
    assert!(store.sticker_pack_request(&[9], &absent).is_err());
    let mut own = key.clone();
    own.author = owner.peer_id();
    assert!(store.sticker_pack_request(&[9], &own).is_err());
    let wrong = PackRequest { pack_id: [99; 32] };
    assert!(store
        .stage_sticker_pack_request(b"x", &[], &[9], &key, &wrong)
        .is_err());
    store.set_blocked(&peer, true).unwrap();
    assert!(matches!(
        store.stage_sticker_pack_request(b"x", &[], &[9], &key, &request),
        Err(Error::BlockedPeer)
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
        store.stage_sticker_pack_request(b"x", &[], &[9], &key, &request),
        Err(Error::ChatClosed)
    ));
    store
        .db
        .execute(
            "UPDATE `conversations` SET `state` = 3 WHERE `peer_id` = ?",
            [&peer],
        )
        .unwrap();
    store.clear_chat_pair(&peer).unwrap();
    assert!(store
        .stage_sticker_pack_request(b"x", &[], &[9], &key, &request)
        .is_err());
    assert_eq!(count(&store), 0);
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
}

#[test]
fn parallel_last_slot_stays_bounded_and_retry_works_at_capacity() {
    let (mut store, owner, path, peer) = database();
    let mut first = None;
    for id in 1..=63 {
        let key = selected(&mut store, &peer, id);
        let request = store.sticker_pack_request(&[9], &key).unwrap();
        let pending = store
            .stage_sticker_pack_request(b"initial", &[], &[9], &key, &request)
            .unwrap();
        if id == 1 {
            first = Some((key, request, pending));
        }
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut jobs = Vec::new();
    for id in [64, 65] {
        let key = selected(&mut store, &peer, id);
        let request = store.sticker_pack_request(&[9], &key).unwrap();
        let mut other = Store::open(&path, &owner).unwrap();
        let barrier = barrier.clone();
        jobs.push(std::thread::spawn(move || {
            barrier.wait();
            match other.stage_sticker_pack_request(b"last", &[], &[9], &key, &request) {
                Ok(_) => true,
                Err(Error::PackRequestsFull) => false,
                Err(error) => panic!("unexpected reservation failure: {error}"),
            }
        }));
    }
    barrier.wait();
    let admitted = jobs
        .into_iter()
        .map(|job| usize::from(job.join().unwrap()))
        .sum::<usize>();
    assert_eq!(admitted, 1);
    assert_eq!(count(&store), 64);
    let (key, request, pending) = first.unwrap();
    assert!(
        store
            .stage_sticker_pack_request(b"retry", &[], &[9], &key, &request)
            .unwrap()
            == pending
    );
    let next = selected(&mut store, &peer, 66);
    let request = store.sticker_pack_request(&[9], &next).unwrap();
    assert!(matches!(
        store.stage_sticker_pack_request(b"overflow", &[], &[9], &next, &request),
        Err(Error::PackRequestsFull)
    ));
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"retry");
    assert_eq!(count(&store), 64);
}
