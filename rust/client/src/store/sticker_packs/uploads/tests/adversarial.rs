use super::*;

#[test]
fn descriptor_validation_and_corruption_fail_without_replacing_the_row() {
    let (mut store, _, _) = database();
    let good = candidate(1, 100);
    let mut invalid = good.clone();
    invalid.release_secret = [0; 32];
    assert!(store.plan_sticker_upload(&invalid, 100).is_err());
    invalid = good.clone();
    invalid.width = 513;
    assert!(store.plan_sticker_upload(&invalid, 100).is_err());
    invalid = good.clone();
    invalid.mime = "image/jpeg".into();
    assert!(store.plan_sticker_upload(&invalid, 100).is_err());
    assert!(store
        .plan_sticker_upload(&candidate(2, 262_145), 100)
        .is_err());
    assert_eq!(used(&store), 0);
    upload(store.plan_sticker_upload(&good, 100).unwrap());
    store
        .db
        .execute(
            "UPDATE `sticker_uploads` SET `charge` = `charge` + 1 WHERE `hash` = ?",
            [good.plaintext_sha256.as_slice()],
        )
        .unwrap();
    assert!(store.plan_sticker_upload(&good, 101).is_err());
    store
        .db
        .execute(
            "UPDATE `sticker_uploads` SET `descriptor` = ?2 WHERE `hash` = ?1",
            params![good.plaintext_sha256.as_slice(), vec![14_u8]],
        )
        .unwrap();
    assert!(store.plan_sticker_upload(&good, 102).is_err());
    let raw: Vec<u8> = store
        .db
        .query_row(
            "SELECT `descriptor` FROM `sticker_uploads` WHERE `hash` = ?",
            [good.plaintext_sha256.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(raw, [14]);
}

#[test]
fn parallel_reservations_cannot_both_spend_the_last_budget() {
    let (mut store, owner, path) = database();
    for index in 0..12 {
        upload(
            store
                .plan_sticker_upload(&candidate(index, 262_144), 100)
                .unwrap(),
        );
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut jobs = Vec::new();
    for index in [20, 21] {
        let mut connection = Store::open(&path, &owner).unwrap();
        let barrier = barrier.clone();
        jobs.push(std::thread::spawn(move || {
            let candidate = candidate(index, 208_883);
            barrier.wait();
            matches!(
                connection.plan_sticker_upload(&candidate, 100).unwrap(),
                UploadPlan::Upload(_)
            )
        }));
    }
    barrier.wait();
    let admitted = jobs
        .into_iter()
        .map(|job| usize::from(job.join().unwrap()))
        .sum::<usize>();
    assert_eq!(admitted, 1);
    assert_eq!(used(&store), 16_777_215);
}

#[test]
fn late_retry_renews_uncertainty_and_confirmation_is_not_a_renewal() {
    let (mut store, _, _) = database();
    let candidate = candidate(1, 100);
    let first = upload(store.plan_sticker_upload(&candidate, 100).unwrap());
    let retry_at = 100 + 7 * 86400;
    let retried = upload(store.plan_sticker_upload(&candidate, retry_at).unwrap());
    assert!(first == retried);
    // Past the original reservation's expiry, a retry is still charged.
    let later = upload(
        store
            .plan_sticker_upload(&candidate, retry_at + 2 * 86400)
            .unwrap(),
    );
    assert!(later == first);
    let confirmed_at = retry_at + 2 * 86400;
    store.finish_sticker_upload(&later, confirmed_at).unwrap();
    assert!(store
        .finish_sticker_upload(&later, confirmed_at + 1)
        .is_err());
    let expires: i64 = store
        .db
        .query_row(
            "SELECT `expires` FROM `sticker_uploads` WHERE `hash` = ?",
            [candidate.plaintext_sha256.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(expires, i64::try_from(confirmed_at).unwrap() + 7 * 86400);
}
