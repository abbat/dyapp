use super::*;

#[test]
fn keep_put_get_and_drop() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let (a, b) = (vec![1u8; 100], vec![2u8; 300]);
    let (ha, hb) = (
        dyapp_identity::sha256(&a).to_vec(),
        dyapp_identity::sha256(&b).to_vec(),
    );

    assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::NotListed);
    let both = [ha.clone(), hb.clone()];
    assert_eq!(
        media.keep("alice", 1, &both, 10).unwrap(),
        Some(both.to_vec())
    );
    assert_eq!(media.keep("alice", 1, &[], 10).unwrap(), None, "stale");
    assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::Stored);
    assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::Stored, "repeat");
    assert_eq!(
        media.put("alice", &a, 0).unwrap(),
        Put::OverQuota,
        "lowered quota"
    );
    assert_eq!(media.put("alice", &b, 350).unwrap(), Put::OverQuota);
    assert_eq!(media.get(&ha).unwrap(), Some(a.clone()));
    assert_eq!(media.get(&hb).unwrap(), None);

    // Bob lists the same blob: stored once, kept while either lists it.
    assert_eq!(
        media.keep("bob", 5, std::slice::from_ref(&ha), 10).unwrap(),
        Some(vec![])
    );
    assert_eq!(media.put("bob", &a, 1000).unwrap(), Put::Stored);
    assert_eq!(media.usage().unwrap().0, 100);
    assert_eq!(
        media
            .keep("alice", 2, std::slice::from_ref(&hb), 20)
            .unwrap(),
        Some(vec![hb])
    );
    assert_eq!(media.get(&ha).unwrap(), Some(a));
    assert_eq!(media.keep("bob", 6, &[], 20).unwrap(), Some(vec![]));
    assert_eq!(media.get(&ha).unwrap(), None);
    assert_eq!(media.usage().unwrap().0, 0);
}

#[test]
fn keep_refresh_uses_signed_time_and_preserves_the_list() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let hashes = vec![vec![1; 32]];
    assert_eq!(
        media.keep("alice", 1, &hashes, 100).unwrap(),
        Some(hashes.clone())
    );
    assert_eq!(media.keep("alice", 1, &hashes, 100).unwrap(), None);
    assert_eq!(media.keep("alice", 1, &hashes, 90).unwrap(), None);
    assert_eq!(
        media.keep("alice", 1, &hashes, 110).unwrap(),
        Some(hashes.clone())
    );
    assert_eq!(media.keep("alice", 1, &[], 120).unwrap(), None);
    let db = lock(&media.db).unwrap();
    let seen: i64 = db
        .query_row(
            "SELECT `last_seen` FROM `owners` WHERE `owner` = 'alice'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(seen, 110);
}

#[test]
fn expiry_keeps_shared_and_attached_blobs_and_honours_refresh() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let data = vec![1; 100];
    let hash = dyapp_identity::sha256(&data).to_vec();
    let hashes = std::slice::from_ref(&hash);
    media.keep("alice", 1, hashes, 100).unwrap();
    media.keep("bob", 1, hashes, 110).unwrap();
    media.put("alice", &data, 1000).unwrap();
    // The single upload fulfils both lists and is counted while either owner holds it.
    assert_eq!(media.keep("bob", 1, hashes, 111).unwrap(), Some(vec![]));
    assert_eq!(media.expire_owners(100).unwrap(), 1);
    assert_eq!(media.get(&hash).unwrap(), Some(data.clone()));
    media.keep("bob", 1, hashes, 150).unwrap();
    assert_eq!(media.expire_owners(120).unwrap(), 0);
    let release = dyapp_identity::sha256(b"secret");
    assert_eq!(
        media
            .attach("sender", &release, hashes, 1000, 1, 10)
            .unwrap(),
        Attached::Missing(vec![])
    );
    assert_eq!(media.expire_owners(150).unwrap(), 2);
    assert_eq!(media.get(&hash).unwrap(), Some(data));
    assert_eq!(media.expire(1000).unwrap(), 1);
    assert_eq!(media.get(&hash).unwrap(), None);
}

#[test]
fn eviction_drops_attachments_then_oldest_keeps_and_stops_at_target() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let blobs: Vec<Vec<u8>> = (1..=4).map(|byte| vec![byte; 100]).collect();
    let hashes: Vec<Vec<u8>> = blobs
        .iter()
        .map(|data| dyapp_identity::sha256(data).to_vec())
        .collect();
    for (owner, index, time) in [("old", 0, 100), ("young", 1, 200)] {
        media
            .keep(owner, 1, std::slice::from_ref(&hashes[index]), time)
            .unwrap();
        media.put(owner, &blobs[index], 1000).unwrap();
    }
    for (index, created) in [(2, 30), (3, 10), (0, 5)] {
        let release = vec![index as u8; 32];
        media
            .attach(
                "sender",
                &release,
                std::slice::from_ref(&hashes[index]),
                1000,
                created,
                10,
            )
            .unwrap();
        media.put("sender", &blobs[index], 1000).unwrap();
    }
    assert_eq!(media.usage().unwrap().0, 400);
    media.evict(380).unwrap(); // 95% of 400; shared attachment frees no bytes.
    assert_eq!(media.usage().unwrap().0, 300);
    assert_eq!(media.get(&hashes[3]).unwrap(), None);
    assert_eq!(media.get(&hashes[2]).unwrap(), Some(blobs[2].clone()));
    assert_eq!(media.get(&hashes[0]).unwrap(), Some(blobs[0].clone()));
    media.evict(100).unwrap();
    assert_eq!(media.usage().unwrap().0, 100);
    assert_eq!(media.get(&hashes[0]).unwrap(), None);
    assert_eq!(media.get(&hashes[2]).unwrap(), None);
    assert_eq!(media.get(&hashes[1]).unwrap(), Some(blobs[1].clone()));
}

#[test]
fn assembled_blobs_expire_when_idle_and_go_first_under_the_disk_guard() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let attached = vec![1u8; 100];
    let hash = dyapp_identity::sha256(&attached).to_vec();
    media
        .attach(
            "sender",
            &[9; 32],
            std::slice::from_ref(&hash),
            1000,
            10,
            10,
        )
        .unwrap();
    media.put("sender", &attached, 1000).unwrap();
    let (old, recent) = ([2u8; 32], [3u8; 32]);
    media.cache(&old, &[2; 50], 100).unwrap();
    media.cache(&recent, &[3; 50], 100).unwrap();
    assert_eq!(media.cached(&recent, 200).unwrap(), Some(vec![3; 50]));
    assert_eq!(media.usage().unwrap().0, 200);
    assert_eq!(media.expire_cache(150).unwrap(), 1);
    assert_eq!(media.cached(&old, 300).unwrap(), None);
    assert_eq!(media.usage().unwrap().0, 150);

    media.cache(&old, &[2; 50], 100).unwrap();
    media.evict(100).unwrap();
    assert_eq!(media.cached(&old, 300).unwrap(), None);
    assert_eq!(media.cached(&recent, 300).unwrap(), None);
    assert_eq!(media.get(&hash).unwrap(), Some(attached), "attachment kept");

    media.cache(&old, &[2; 50], 400).unwrap();
    assert!(media.purge(&old).unwrap());
    assert_eq!(media.cached(&old, 500).unwrap(), None);
    assert_eq!(media.usage().unwrap().0, 100);
}

#[test]
fn attach_put_release_and_expire() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let (a, b) = (vec![1u8; 100], vec![2u8; 300]);
    let (ha, hb) = (
        dyapp_identity::sha256(&a).to_vec(),
        dyapp_identity::sha256(&b).to_vec(),
    );
    let (s1, s2) = (b"one".as_slice(), b"two".as_slice());
    let (r1, r2) = (dyapp_identity::sha256(s1), dyapp_identity::sha256(s2));

    let ha1 = std::slice::from_ref(&ha);
    assert_eq!(
        media.attach("alice", &r1, ha1, 100, 1, 1).unwrap(),
        Attached::Missing(vec![ha.clone()])
    );
    assert_eq!(
        media.attach("alice", &r2, ha1, 100, 1, 1).unwrap(),
        Attached::Full,
        "max"
    );
    assert_eq!(
        media.attach("alice", &r1, ha1, 100, 1, 1).unwrap(),
        Attached::Missing(vec![ha.clone()])
    );
    assert_eq!(media.put("bob", &a, 1000).unwrap(), Put::NotListed);
    assert_eq!(media.put("alice", &a, 1000).unwrap(), Put::Stored);
    // Kept and attached: counted for both.
    media
        .keep("alice", 1, std::slice::from_ref(&hb), 10)
        .unwrap();
    assert_eq!(media.put("alice", &b, 350).unwrap(), Put::OverQuota);
    assert_eq!(media.put("alice", &b, 400).unwrap(), Put::Stored);
    assert_eq!(media.usage().unwrap().0, 400);

    assert!(!media.release(s2).unwrap());
    assert!(media.release(s1).unwrap());
    assert_eq!(media.get(&ha).unwrap(), None);
    assert_eq!(media.get(&hb).unwrap(), Some(b.clone()));

    // An attached blob that is also kept survives its expiry.
    let hb1 = std::slice::from_ref(&hb);
    assert_eq!(
        media.attach("alice", &r2, hb1, 50, 2, 1).unwrap(),
        Attached::Missing(vec![])
    );
    assert_eq!(media.put("alice", &b, 1000).unwrap(), Put::Stored);
    assert_eq!(media.expire(49).unwrap(), 0);
    assert_eq!(media.expire(50).unwrap(), 1);
    assert_eq!(media.get(&hb).unwrap(), Some(b));
    assert!(!media.release(s2).unwrap());
}

#[test]
fn a_released_attachment_is_refused_until_its_expiry() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let data = vec![1u8; 100];
    let hashes = [dyapp_identity::sha256(&data).to_vec()];
    let release = dyapp_identity::sha256(b"secret");
    let attach = || {
        media
            .attach("alice", &release, &hashes, 100, 1, 10)
            .unwrap()
    };
    assert_eq!(attach(), Attached::Missing(hashes.to_vec()));
    assert_eq!(media.put("alice", &data, 1000).unwrap(), Put::Stored);
    assert!(media.release(b"secret").unwrap());

    assert_eq!(attach(), Attached::Released);
    assert_eq!(media.put("alice", &data, 1000).unwrap(), Put::NotListed);
    assert_eq!(media.get(&hashes[0]).unwrap(), None);
    assert!(!media.release(b"secret").unwrap());
    assert_eq!(media.expire(99).unwrap(), 0);
    assert_eq!(attach(), Attached::Released);
    // At the attachment's expiry the release record goes too.
    media.expire(100).unwrap();
    assert_eq!(attach(), Attached::Missing(hashes.to_vec()));
}

#[test]
fn keeps_after_resumes_skips_unsigned_and_uses_an_index() {
    let dir = PathBuf::from(format!("/tmp/ai/test-media-{}", uuid::Uuid::new_v4()));
    let media = MediaStore::open(&dir).unwrap();
    let record = |n: u8| SignedRecord {
        payload: vec![n],
        ..SignedRecord::default()
    };
    let keep = proto::MediaKeep {
        version: 1,
        time: 10,
        ..Default::default()
    };
    media.keep_signed("a", &keep, &record(1)).unwrap();
    media.keep("b", 1, &[], 10).unwrap();
    media.keep_signed("c", &keep, &record(3)).unwrap();

    let all = media.keeps_after("", 10).unwrap();
    assert_eq!(
        all,
        vec![("a".to_string(), record(1)), ("c".to_string(), record(3))],
        "unsigned b skipped"
    );
    assert_eq!(media.keeps_after("a", 1).unwrap(), all[1..]);
    assert!(media.keeps_after("c", 10).unwrap().is_empty());
    crate::storage::assert_indexed(&lock(&media.db).unwrap(), KEEPS_AFTER);
}
