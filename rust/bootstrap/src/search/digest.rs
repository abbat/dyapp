use super::tests::{assert_digest_consistent, fill, index, profile, signed};
use super::*;
use dyapp_identity::Identity;

#[test]
fn digest_follows_mutations_and_rejections() {
    let index = index();
    let keys = fill(&index, 40, 0);
    let owner = Identity::generate();
    let (key, p, record) = signed(&owner, profile(1));
    index.put(&key, &p, &record, 1000, true).unwrap();
    assert_digest_consistent(&index);
    assert_eq!(
        index.put(&key, &p, &record, 2000, true).unwrap(),
        Put::Stale
    );
    let (_, p, record) = signed(&owner, profile(2));
    assert!(matches!(
        index.put(&key, &p, &record, 1001, true).unwrap(),
        Put::Limited { .. }
    ));
    assert_digest_consistent(&index);
    index.put(&key, &p, &record, 1600, true).unwrap();
    assert_digest_consistent(&index);
    let (_, p, record) = signed(&owner, Profile::tombstone(3, 0, 0));
    index.put(&key, &p, &record, 2300, true).unwrap();
    assert_digest_consistent(&index);
    index.heartbeat(&keys[0], 1, 5000).unwrap();
    assert_digest_consistent(&index);
    index.expire(2500).unwrap();
    assert_digest_consistent(&index);
    index.expire(i64::MAX).unwrap();
    assert_digest_consistent(&index);
}

#[test]
fn slots_and_inventory_range_boundaries() {
    let mut key = [0; 32];
    key[..3].copy_from_slice(&[0x5a, 0xbc, 0xde]);
    assert_eq!(slot(&key), 0x5bcd);
    assert!(slot_start(topic(&key), slot(&key)) <= key);
    assert!(key < slot_start(topic(&key), slot(&key) + 1));
    for topic in 0..SEARCH_TOPICS {
        for p in [0, 1, 255, 256, 4095, 4096, SLOTS - 1] {
            assert_eq!(slot(&slot_start(topic, p)), p);
            assert_eq!(super::topic(&slot_start(topic, p)), topic);
        }
    }
    let index = index();
    let (_, p, record) = signed(&Identity::generate(), profile(1));
    let keys: Vec<_> = [0, 1, 255, 256, SLOTS - 1]
        .map(|p| slot_start(3, p))
        .into_iter()
        .collect();
    for key in &keys {
        index.put(key, &p, &record, 0, false).unwrap();
    }
    let all = index.inventory(3, &[], &[], 10_000).unwrap();
    assert_eq!(all.iter().map(|(key, _)| *key).collect::<Vec<_>>(), keys);
    assert_eq!(index.inventory(3, &[], &keys[2], 10_000).unwrap(), all[..2]);
    assert_eq!(
        index.inventory(3, &keys[0], &keys[3], 10_000).unwrap(),
        all[1..3]
    );
    assert!(index
        .inventory(3, &keys[2], &keys[2], 10_000)
        .unwrap()
        .is_empty());
    assert_eq!(index.inventory(3, &[], &[], 0).unwrap().len(), 0);
    assert_digest_consistent(&index);
    let (sub, count) = index.sub_digest(3, 255).unwrap();
    assert_eq!(count, 1);
    assert_eq!(sub[255], row_hash(&keys[4], 1));
    assert!(index.sub_digest(SEARCH_TOPICS, 0).is_err());
}
