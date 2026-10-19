//! Search index tests.
use super::*;
use dyapp_identity::Identity;

pub(super) fn index() -> SearchIndex {
    sized(1 << 40)
}

pub(super) fn sized(max_bytes: u64) -> SearchIndex {
    let dir = format!("/tmp/ai/test-search-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    SearchIndex::open(&Path::new(&dir).join("profiles-idx.db"), max_bytes).unwrap()
}

pub(super) fn signed(identity: &Identity, profile: Profile) -> ([u8; 32], Profile, SignedRecord) {
    let record = profile.sign(identity);
    let key = dyapp_identity::key_hash(&identity.public_key());
    (key, profile, record)
}

pub(super) fn profile(version: u64) -> Profile {
    Profile {
        version,
        age: 30,
        country: "DE".into(),
        place: "berlin".into(),
        interests: vec!["chess".into()],
        ..Profile::default()
    }
}

pub(super) fn fill(index: &SearchIndex, n: u64, from: i64) -> Vec<[u8; 32]> {
    (0..n)
        .map(|i| {
            let (key, p, r) = signed(&Identity::generate(), profile(1));
            let now = from + i64::try_from(i).unwrap();
            index.put(&key, &p, &r, now, true).unwrap();
            key
        })
        .collect()
}

pub(super) fn assert_digest_consistent(index: &SearchIndex) {
    for topic in 0..SEARCH_TOPICS {
        let rows = index.inventory(topic, &[], &[], u32::MAX).unwrap();
        let mut expected = vec![[0; 32]; 256];
        for (key, version) in &rows {
            xor(
                &mut expected[(slot(key) >> 8) as usize],
                &row_hash(key, *version),
            );
        }
        assert_eq!(index.digest(topic).unwrap(), expected);
        for bucket in 0..=255u8 {
            let (sub, scanned) = index.sub_digest(topic, bucket).unwrap();
            let mut sum = [0; 32];
            for hash in sub {
                xor(&mut sum, &hash);
            }
            assert_eq!(sum, expected[usize::from(bucket)]);
            assert_eq!(
                scanned,
                rows.iter()
                    .filter(|(key, _)| slot(key) >> 8 == u32::from(bucket))
                    .count()
            );
        }
    }
}

#[test]
fn digest_rebuilt_on_open() {
    let index = index();
    fill(&index, 20, 0);
    let path = index.path.clone();
    lock(&index.db)
        .unwrap()
        .execute("UPDATE `digest` SET `hash` = ?", [vec![1u8; 32]])
        .unwrap();
    drop(index);
    let index = SearchIndex::open(&path, u64::MAX).unwrap();
    assert_digest_consistent(&index);
    // Older binaries do not update the derived table.
    lock(&index.db)
        .unwrap()
        .execute("DROP TABLE `digest`", [])
        .unwrap();
    drop(index);
    assert_digest_consistent(&SearchIndex::open(&path, u64::MAX).unwrap());
}
