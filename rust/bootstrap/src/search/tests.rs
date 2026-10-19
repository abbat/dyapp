//! Search index tests.
use super::*;
use dyapp_identity::Identity;

pub(super) fn index() -> SearchIndex {
    sized(1 << 40)
}

pub(super) fn sized(max_bytes: u64) -> SearchIndex {
    quota(max_bytes, 0.10)
}

pub(super) fn quota(max_bytes: u64, newcomer_share: f64) -> SearchIndex {
    let dir = format!("/tmp/ai/test-search-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let search = crate::config::Search {
        max_bytes,
        newcomer_share,
        ..crate::config::Search::default()
    };
    SearchIndex::open(&Path::new(&dir).join("profiles-idx.db"), &search).unwrap()
}

/// Puts `n` new keys at `place` from `now` on; `limit = false` inside the first
/// `newcomer_days` stores established keys.
pub(super) fn put_at(
    index: &SearchIndex,
    place: &str,
    n: usize,
    now: i64,
    limit: bool,
) -> Vec<[u8; 32]> {
    (0..n)
        .map(|i| {
            let profile = Profile {
                place: place.into(),
                ..profile(1)
            };
            let (key, p, r) = signed(&Identity::generate(), profile);
            let at = now + i64::try_from(i).unwrap();
            assert_eq!(index.put(&key, &p, &r, at, limit).unwrap(), Put::Stored);
            key
        })
        .collect()
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
        place: "2950159".into(),
        interests: vec!["chess".into()],
        ..Profile::default()
    }
}

pub(super) type Counts = Vec<(String, String, i64)>;

impl SearchIndex {
    fn rows(&self, sql: &str) -> Counts {
        let db = lock(&self.db).unwrap();
        let mut statement = db.prepare(sql).unwrap();
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }

    pub(super) fn place_counts(&self) -> Counts {
        self.rows("SELECT * FROM `place_count` ORDER BY 1, 2")
    }

    pub(super) fn recount(&self) -> Counts {
        self.rows("SELECT `country`, `place`, COUNT(*) FROM `profile` GROUP BY 1, 2 ORDER BY 1, 2")
    }

    pub(super) fn column<T: rusqlite::types::FromSql>(&self, sql: &str, key: &[u8; 32]) -> T {
        lock(&self.db)
            .unwrap()
            .query_row(sql, [&key[..]], |row| row.get(0))
            .unwrap()
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
    let index = SearchIndex::open(&path, &crate::config::Search::default()).unwrap();
    assert_digest_consistent(&index);
    // Older binaries do not update the derived table.
    lock(&index.db)
        .unwrap()
        .execute("DROP TABLE `digest`", [])
        .unwrap();
    drop(index);
    assert_digest_consistent(&SearchIndex::open(&path, &crate::config::Search::default()).unwrap());
}

#[test]
fn unknown_places_go_to_the_country_bucket() {
    let index = index();
    let mut keys = vec![];
    for place in ["2950159", "999999999", "berlin", ""] {
        let profile = Profile {
            place: place.into(),
            ..profile(1)
        };
        let (key, p, r) = signed(&Identity::generate(), profile);
        index.put(&key, &p, &r, 1_000, true).unwrap();
        keys.push(key);
    }
    let stored: Vec<String> = keys
        .iter()
        .map(|k| index.column("SELECT `place` FROM `profile` WHERE `key` = ?", k))
        .collect();
    assert_eq!(stored, ["2950159", "", "", ""]);
    let expected: Counts = vec![
        ("DE".into(), "".into(), 3),
        ("DE".into(), "2950159".into(), 1),
    ];
    assert_eq!(index.place_counts(), expected);
}

#[test]
fn place_counts_follow_put_remove_and_expiry() {
    let index = index();
    let id = Identity::generate();
    let (key, p, r) = signed(&id, profile(1));
    index.put(&key, &p, &r, 1_000, true).unwrap();
    let moved = Profile {
        place: "2911298".into(),
        ..profile(2)
    };
    let (_, p, r) = signed(&id, moved);
    index.put(&key, &p, &r, 2_000, true).unwrap();
    assert_eq!(index.place_counts(), index.recount());
    assert_eq!(index.place_counts().len(), 1);
    let (_, p, r) = signed(&id, Profile::tombstone(3, 0, 0));
    index.put(&key, &p, &r, 3_000, true).unwrap();
    assert_eq!(index.place_counts(), vec![]);
    fill(&index, 10, 4_000);
    assert_eq!(index.place_counts(), index.recount());
    index.expire(i64::MAX).unwrap();
    assert_eq!(index.place_counts(), vec![]);
}

#[test]
fn first_seen_and_bytes_are_kept() {
    let index = index();
    let id = Identity::generate();
    let (key, p, r) = signed(&id, profile(1));
    index.put(&key, &p, &r, 1_000, true).unwrap();
    let (_, p, r) = signed(&id, profile(2));
    index.put(&key, &p, &r, 5_000, true).unwrap();
    let first_seen: i64 = index.column("SELECT `first_seen` FROM `key` WHERE `key` = ?", &key);
    let bytes: i64 = index.column("SELECT `bytes` FROM `key` WHERE `key` = ?", &key);
    assert_eq!(first_seen, 1_000);
    assert_eq!(bytes, r.encoded_len() as i64);
}

#[test]
fn migration_from_version_zero() {
    let dir = format!("/tmp/ai/test-search-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let path = Path::new(&dir).join("profiles-idx.db");
    {
        let db = Connection::open(&path).unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db.execute(
            "INSERT INTO `profile` (`key`, `topic`, `version`, `country`, `place`, `r`)
               VALUES (?1, 0, 1, 'DE', 'berlin', 5), (?2, 0, 1, 'DE', '2950159', 6)",
            [[1u8; 32], [2; 32]],
        )
        .unwrap();
        db.execute(
            "INSERT INTO `key` (`key`, `topic`, `version`, `deleted`, `updated`, `heartbeat`,
               `beat_day`, `beats`) VALUES (?, 0, 1, 0, 0, 0, 0, 0)",
            [[1u8; 32]],
        )
        .unwrap();
    }
    let index = SearchIndex::open(&path, &crate::config::Search::default()).unwrap();
    let expected: Counts = vec![
        ("DE".into(), "".into(), 1),
        ("DE".into(), "2950159".into(), 1),
    ];
    assert_eq!(index.place_counts(), expected);
    assert!(index.created() > 0);
}
