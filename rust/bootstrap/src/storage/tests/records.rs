//! Envelope, eviction, signed profile, cursor and maintenance tests of the store.

use super::*;

#[test]
fn maintenance_frees_pages_in_bounded_steps() {
    let dir = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
    let store = BootstrapStore::new(&dir).unwrap();
    let record = SignedRecord {
        payload: vec![7; 10_000],
        ..SignedRecord::default()
    };
    for id in 0..500u32 {
        store
            .put_plain(&[1; 32], &id.to_be_bytes(), &record, false, 0, u64::MAX)
            .unwrap();
    }
    assert_eq!(store.cleanup_expired(1).unwrap(), 500);
    store.maintain(1).unwrap();
    let size = || {
        std::fs::metadata(format!("{dir}/messages.db"))
            .unwrap()
            .len()
    };
    let before = size();
    let [_, free] = store.maintain(10).unwrap();
    let [_, after] = store.maintain(10).unwrap();
    assert!(free > 1000);
    assert_eq!(after, free - 10);
    let (used, free) = store.messages_usage().unwrap();
    assert!(used < 100_000 && free > 0, "{used} {free}");
    assert_eq!(store.maintain(u32::MAX).unwrap()[1], 0);
    assert!(size() < before / 10, "file did not shrink");
    assert_eq!(
        std::fs::metadata(format!("{dir}/messages.db-wal"))
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn eviction_takes_the_oldest_first() {
    let store = temp_store();
    let record = SignedRecord {
        payload: vec![7; 10_000],
        ..SignedRecord::default()
    };
    for id in 0..200u32 {
        store
            .put_plain(&[1; 32], &id.to_be_bytes(), &record, false, 0, u64::MAX)
            .unwrap();
    }
    let target = store.messages_usage().unwrap().0 / 2;
    let evicted = store.evict_envelopes(target).unwrap();
    assert!((64..200).contains(&evicted), "{evicted}");
    assert!(store.messages_usage().unwrap().0 < target);
    let (left, _) = store.fetch_envelopes(&[1; 32], 1000, u64::MAX).unwrap();
    assert_eq!(left.len(), 200 - evicted);
    // The newest is kept; asking again below the target deletes nothing.
    assert!(store
        .put_plain(&[1; 32], &199u32.to_be_bytes(), &record, false, 0, 0)
        .unwrap());
    assert_eq!(store.evict_envelopes(u64::MAX).unwrap(), 0);

    // A republished profile moves to the end of the eviction order.
    let owners: Vec<_> = (0..3).map(|_| Identity::generate()).collect();
    let publish = |owner: &Identity, version| {
        let profile = Profile {
            version,
            ..Profile::default()
        };
        store.put_profile(&profile.sign(owner)).unwrap();
    };
    for owner in &owners {
        publish(owner, 1);
    }
    publish(&owners[0], 2);
    let order: Vec<String> = lock(&store.profiles)
        .unwrap()
        .prepare("SELECT `peer_id` FROM `profiles` ORDER BY `rowid`")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    let ids: Vec<String> = [1, 2, 0].iter().map(|&i| owners[i].peer_id()).collect();
    assert_eq!(order, ids);
}

#[test]
fn envelopes_once_per_id_limits_and_expiry() {
    let store = temp_store();
    let record = |n: u8| SignedRecord {
        payload: vec![n; 100],
        ..SignedRecord::default()
    };
    let size = record(0).encode_to_vec().len() as u64;
    let (a, b) = ([1u8; 32], [2u8; 32]);

    assert!(store
        .put_plain(&a, &[1], &record(1), false, 2000, 2 * size)
        .unwrap());
    assert!(store
        .put_plain(&a, &[1], &record(9), false, 2000, 2 * size)
        .unwrap());
    assert!(store
        .put_plain(&a, &[2], &record(2), false, 3000, 2 * size)
        .unwrap());
    // Full, but a repeated put still succeeds.
    assert!(!store
        .put_plain(&a, &[3], &record(3), false, 2000, 2 * size)
        .unwrap());
    assert!(store
        .put_plain(&a, &[2], &record(2), false, 2000, 2 * size)
        .unwrap());
    assert!(store
        .put_plain(&b, &[1], &record(4), false, 2000, size)
        .unwrap());

    let (all, more) = store.fetch_envelopes(&a, 10, u64::MAX).unwrap();
    assert_eq!((all, more), (vec![record(1), record(2)], false));
    assert_eq!(
        store.fetch_envelopes(&a, 1, u64::MAX).unwrap(),
        (vec![record(1)], true)
    );
    assert_eq!(
        store.fetch_envelopes(&a, 10, 1).unwrap(),
        (vec![record(1)], true)
    );

    store.ack_envelopes(&a, &[vec![1], vec![7]]).unwrap();
    assert_eq!(
        store.fetch_envelopes(&a, 10, u64::MAX).unwrap().0,
        vec![record(2)]
    );
    assert_eq!(store.cleanup_expired(2000).unwrap(), 1);
    assert_eq!(
        store.fetch_envelopes(&a, 10, u64::MAX).unwrap().0,
        vec![record(2)]
    );
    assert_eq!(
        store.fetch_envelopes(&b, 10, u64::MAX).unwrap().0,
        [] as [dyapp_identity::SignedRecord; 0]
    );
}

#[test]
fn paged_fetch_moves_past_unacked_head_and_deleted_cursor() {
    let dir = format!("build/test-bootstrap-{}", Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let store = BootstrapStore::new(&dir).unwrap();
    let mailbox = [7u8; 32];
    let record = |n: u8| SignedRecord {
        payload: vec![n; 32],
        ..SignedRecord::default()
    };
    // A signal inserted first must still come after all ordinary messages.
    assert!(store
        .put_plain(&mailbox, &[250; 16], &record(250), true, 3000, 1 << 20)
        .unwrap());
    for n in 0..=100u8 {
        assert!(store
            .put_plain(&mailbox, &[n; 16], &record(n), false, 3000, 1 << 20)
            .unwrap());
    }
    let (first, more, cursor) = store
        .fetch_envelopes_after(&mailbox, None, 100, u64::MAX)
        .unwrap();
    assert_eq!(first.len(), 100);
    assert_eq!(first[0], record(0)); // Deliberately left unacked.
    assert_eq!(first[99], record(99));
    assert!(more);
    store.ack_envelopes(&mailbox, &[vec![99; 16]]).unwrap();
    let (last, more, next) = store
        .fetch_envelopes_after(&mailbox, cursor, 100, u64::MAX)
        .unwrap();
    assert_eq!(last, vec![record(100), record(250)]);
    assert!(!more);
    assert!(next > cursor);
}

/// Ids of a mailbox's envelopes in arrival order.
fn ids(store: &BootstrapStore, mailbox: &[u8]) -> Vec<Vec<u8>> {
    lock(&store.messages)
        .unwrap()
        .prepare("SELECT `id` FROM `envelopes` WHERE `mailbox` = ? ORDER BY `seq`")
        .unwrap()
        .query_map([mailbox], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

#[test]
fn signals_have_their_own_quota_and_evict_the_oldest() {
    let store = temp_store();
    let record = SignedRecord {
        payload: vec![7; 100],
        ..SignedRecord::default()
    };
    let quota = 3 * record.encode_to_vec().len() as u64;
    let (a, b) = ([1u8; 32], [2u8; 32]);
    for i in 0..3 {
        assert!(store.put_plain(&a, &[i], &record, true, 0, quota).unwrap());
    }
    assert!(store.put_plain(&b, &[0], &record, true, 0, quota).unwrap());
    assert!(store.put_plain(&a, &[9], &record, false, 0, quota).unwrap());
    // The fourth signal evicts the oldest signal of mailbox a only.
    assert!(store.put_plain(&a, &[3], &record, true, 0, quota).unwrap());
    assert_eq!(ids(&store, &a), [[1], [2], [9], [3]]);
    assert_eq!(ids(&store, &b).len(), 1);
    // A repeated id is a no-op whatever its class.
    assert!(store.put_plain(&a, &[9], &record, true, 0, quota).unwrap());
    assert_eq!(ids(&store, &a).len(), 4);
}

#[test]
fn signals_never_block_messages_and_come_after_them() {
    let store = temp_store();
    let record = |n: u8| SignedRecord {
        payload: vec![n; 100],
        ..SignedRecord::default()
    };
    let cap = 2 * record(0).encode_to_vec().len() as u64;
    let m = [1u8; 32];
    for i in 0..1000u16 {
        assert!(store
            .put_plain(&m, &i.to_be_bytes(), &record(0), true, 0, u64::MAX)
            .unwrap());
    }
    // Signals do not count against the message cap.
    assert!(store
        .put_plain(&m, &[7], &record(7), false, 0, cap)
        .unwrap());
    assert!(store
        .put_plain(&m, &[8], &record(8), false, 0, cap)
        .unwrap());
    assert!(!store
        .put_plain(&m, &[6], &record(6), false, 0, cap)
        .unwrap());
    assert_eq!(
        store.fetch_envelopes(&m, 2, u64::MAX).unwrap(),
        (vec![record(7), record(8)], true)
    );
    assert_eq!(
        store.fetch_envelopes(&m, 2000, u64::MAX).unwrap().0.len(),
        1002
    );
}

#[test]
fn signed_profile_versions_and_tombstone() {
    let store = temp_store();
    let owner = Identity::generate();
    let profile = |version| Profile {
        version,
        ..Profile::default()
    };

    store.put_profile(&profile(2).sign(&owner)).unwrap();
    assert!(matches!(
        store.put_profile(&profile(1).sign(&owner)),
        Err(BootstrapError::Profile(dyapp_profile::Error::Stale))
    ));
    let mut forged = profile(3).sign(&owner);
    forged.payload[1] ^= 1;
    assert!(store.put_profile(&forged).is_err());

    let stored = store.get_profile(&owner.peer_id()).unwrap().unwrap();
    assert_eq!(dyapp_profile::stored_version(&stored), Ok(2));
    assert_eq!(store.list_profiles(0, 10).unwrap().len(), 1);
    assert_eq!(
        store.list_profiles(1, 10).unwrap(),
        [] as [dyapp_identity::SignedRecord; 0]
    );

    store
        .put_profile(&Profile::tombstone(3, 0, 0).sign(&owner))
        .unwrap();
    assert_eq!(
        store.list_profiles(0, 10).unwrap(),
        [] as [dyapp_identity::SignedRecord; 0]
    );
    assert!(store.put_profile(&profile(2).sign(&owner)).is_err());
    assert!(matches!(
        store.put_profile(&profile(4).sign(&owner)),
        Err(BootstrapError::Profile(dyapp_profile::Error::Invalid(
            "profile already deleted"
        )))
    ));
    assert_eq!(
        dyapp_profile::stored_version(&store.get_profile(&owner.peer_id()).unwrap().unwrap()),
        Ok(3)
    );
    // Finality on a cooperating node lasts only while it retains the tombstone.
    let cutoff = chrono::Utc::now().timestamp();
    assert_eq!(store.expire_profiles(cutoff - 60).unwrap(), 0);
    assert_eq!(store.expire_profiles(cutoff + 1).unwrap(), 1);
    assert!(store.get_profile(&owner.peer_id()).unwrap().is_none());
    store.put_profile(&profile(4).sign(&owner)).unwrap();
}

#[test]
fn cursors_resume_skip_dead_rows_and_use_indexes() {
    let store = temp_store();
    let record = SignedRecord::default();
    for (id, (mailbox, expires_at)) in [(1u8, i64::MAX), (2, 0), (3, i64::MAX), (3, i64::MAX)]
        .into_iter()
        .enumerate()
    {
        store
            .put_plain(
                &[mailbox; 32],
                &[u8::try_from(id).unwrap()],
                &record,
                false,
                expires_at,
                u64::MAX,
            )
            .unwrap();
    }
    assert_eq!(
        store.mailboxes_after(&[], 10).unwrap(),
        vec![vec![1; 32], vec![3; 32]],
        "expired mailbox 2 skipped, 3 once"
    );
    assert_eq!(
        store.mailboxes_after(&[1; 32], 1).unwrap(),
        vec![vec![3; 32]]
    );
    assert_eq!(
        store.mailboxes_after(&[3; 32], 10).unwrap(),
        [] as [std::vec::Vec<u8>; 0]
    );

    let mut owners: Vec<_> = (0..3).map(|_| Identity::generate()).collect();
    owners.sort_by_key(Identity::peer_id);
    for owner in &owners {
        let profile = Profile {
            version: 1,
            ..Profile::default()
        };
        store.put_profile(&profile.sign(owner)).unwrap();
    }
    store
        .put_profile(&Profile::tombstone(2, 0, 0).sign(&owners[1]))
        .unwrap();
    let after = store.profiles_after(&owners[0].peer_id(), 10).unwrap();
    assert_eq!(after.len(), 1, "tombstone skipped");
    assert_eq!(after[0].public_key, owners[2].public_key());
    assert_eq!(store.profiles_after("", 1).unwrap().len(), 1);

    assert_indexed(&lock(&store.messages).unwrap(), MAILBOXES_AFTER);
    assert_indexed(&lock(&store.profiles).unwrap(), PROFILES_AFTER);
}

#[test]
fn data_survives_reopen() {
    let path = format!("/tmp/ai/test-bootstrap-{}", Uuid::new_v4());
    let owner = Identity::generate();
    BootstrapStore::new(&path)
        .unwrap()
        .put_profile(
            &Profile {
                version: 1,
                ..Profile::default()
            }
            .sign(&owner),
        )
        .unwrap();
    let store = BootstrapStore::new(&path).unwrap();
    assert!(store.get_profile(&owner.peer_id()).unwrap().is_some());
}

#[test]
fn test_health_check() {
    assert!(temp_store().health_check().is_ok());
}
