use super::*;
use crate::storage::tests::temp_store;

fn record(len: usize) -> SignedRecord {
    SignedRecord {
        payload: vec![7; len],
        ..SignedRecord::default()
    }
}

/// Encoded size of `record(100)`.
fn small() -> u64 {
    record(100).encode_to_vec().len() as u64
}

/// Where an envelope came from: (token key, via, source group).
type Source<'a> = (Option<&'a [u8]>, &'a [u8], &'a [u8]);

const PLAIN: Source = (None, b"", b"");

fn put(
    store: &BootstrapStore,
    mailbox: u8,
    id: u16,
    signal: bool,
    from: Source,
    max_bytes: u64,
    pool: Option<u64>,
) -> Put {
    put_record(
        store,
        mailbox,
        id,
        &record(100),
        signal,
        from,
        max_bytes,
        pool,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors BootstrapStore::put_envelope for the tests"
)]
fn put_record(
    store: &BootstrapStore,
    mailbox: u8,
    id: u16,
    record: &SignedRecord,
    signal: bool,
    (token_key, via, source_group): Source,
    max_bytes: u64,
    pool: Option<u64>,
) -> Put {
    let mailbox = [mailbox; 32];
    let id = id.to_be_bytes();
    let envelope = NewEnvelope {
        token_key,
        via,
        source_group,
        ..NewEnvelope::new(&mailbox, &id, record, signal, 1000)
    };
    store.put_envelope(&envelope, max_bytes, pool).unwrap()
}

/// Ids of a mailbox's envelopes of `class` in arrival order.
fn ids(store: &BootstrapStore, mailbox: u8, class: i64) -> Vec<u16> {
    lock(&store.messages)
        .unwrap()
        .prepare("SELECT `id` FROM `envelopes` WHERE `mailbox` = ? AND `class` = ? ORDER BY `seq`")
        .unwrap()
        .query_map(params![&[mailbox; 32][..], class], |row| {
            let id: Vec<u8> = row.get(0)?;
            Ok(u16::from_be_bytes([id[0], id[1]]))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// The trigger counters equal the sums over the stored tokenless envelopes.
fn assert_counters(store: &BootstrapStore) {
    let db = lock(&store.messages).unwrap();
    let rows = |sql: &str| -> Vec<(Vec<u8>, i64, i64)> {
        db.prepare(sql)
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(
        rows("SELECT `mailbox`, `class`, `bytes` FROM `tokenless_mailbox` ORDER BY 1, 2"),
        rows(
            "SELECT `mailbox`, `class`, SUM(`size`) FROM `envelopes` WHERE `token_key` IS NULL \
             GROUP BY 1, 2 ORDER BY 1, 2"
        )
    );
    assert_eq!(
        rows("SELECT x'', `class`, `bytes` FROM `tokenless_total` WHERE `bytes` > 0 ORDER BY 2"),
        rows(
            "SELECT x'', `class`, SUM(`size`) FROM `envelopes` WHERE `token_key` IS NULL \
             GROUP BY 2 ORDER BY 2"
        )
    );
}

#[test]
fn a_single_group_spammer_evicts_only_itself() {
    let store = temp_store();
    let pool = Some(4 * small());
    assert_eq!(
        put(&store, 1, 0, false, (None, b"v", b"first"), u64::MAX, pool),
        Put::Stored
    );
    for id in 1..=10 {
        let stored = put(&store, 1, id, false, (None, b"v", b"spam"), u64::MAX, pool);
        assert_eq!(stored, Put::Stored);
    }
    assert_eq!(ids(&store, 1, 0), [0, 8, 9, 10]);
    // Tokened messages count against `max_bytes` only, and the pool never evicts them.
    let key: &[u8] = b"key";
    for id in 11..=13 {
        let stored = put(
            &store,
            1,
            id,
            false,
            (Some(key), b"", b""),
            2 * small(),
            pool,
        );
        assert_eq!(stored, if id < 13 { Put::Stored } else { Put::Full });
    }
    assert_eq!(
        put(&store, 1, 14, false, (None, b"v", b"spam"), u64::MAX, pool),
        Put::Stored
    );
    assert_eq!(ids(&store, 1, 0), [0, 9, 10, 11, 12, 14]);
    // Without a pool tokenless messages count against `max_bytes` with the tokened ones.
    assert_eq!(
        put(&store, 1, 15, false, PLAIN, 6 * small(), None),
        Put::Full
    );
    assert_counters(&store);
}

#[test]
fn many_labels_through_one_via_do_not_evict_another_via() {
    let store = temp_store();
    let pool = Some(4 * small());
    put(&store, 1, 0, false, (None, b"local", b"g"), u64::MAX, pool);
    for id in 1..=1000u16 {
        let group = id.to_be_bytes();
        let stored = put(
            &store,
            1,
            id,
            false,
            (None, b"peer", &group),
            u64::MAX,
            pool,
        );
        assert_eq!(stored, Put::Stored);
    }
    assert_eq!(ids(&store, 1, 0), [0, 998, 999, 1000]);
    assert_counters(&store);
}

#[test]
fn an_incoming_victim_is_dropped_and_nothing_deleted() {
    let store = temp_store();
    let pool = 3 * small();
    for id in 0..3 {
        put(
            &store,
            1,
            id,
            false,
            (None, b"x", b"g"),
            u64::MAX,
            Some(pool),
        );
    }
    // The first victim is taken from via x; then the incoming via y holds most and goes itself.
    let big = record(250);
    let size = big.encode_to_vec().len() as u64;
    assert!(2 * small() < size && size < pool);
    let from = (None, &b"y"[..], &b"g"[..]);
    let dropped = put_record(&store, 1, 9, &big, false, from, u64::MAX, Some(pool));
    assert_eq!(dropped, Put::Dropped);
    assert_eq!(ids(&store, 1, 0), [0, 1, 2]);
    // A message larger than the pool never gets in either.
    let empty = put_record(&store, 2, 0, &big, false, from, u64::MAX, Some(small()));
    assert_eq!(empty, Put::Dropped);
    assert!(ids(&store, 2, 0).is_empty());
    assert_counters(&store);
}

#[test]
fn tokenless_signals_never_evict_tokened_ones() {
    let store = temp_store();
    let max = 3 * small();
    let (k1, k2): (&[u8], &[u8]) = (b"k1", b"k2");
    assert_eq!(
        put(&store, 1, 0, true, (Some(k1), b"", b""), max, None),
        Put::Stored
    );
    for id in 1..=100u16 {
        let group = (id % 7).to_be_bytes();
        let stored = put(&store, 1, id, true, (None, b"v", &group), max, None);
        assert_eq!(stored, Put::Stored);
    }
    assert_eq!(ids(&store, 1, 1)[0], 0);
    assert_eq!(ids(&store, 1, 1).len(), 3);
    // Tokened signals take the tokenless ones first, then the oldest of the busiest key.
    put(&store, 1, 101, true, (Some(k1), b"", b""), max, None);
    put(&store, 1, 102, true, (Some(k2), b"", b""), max, None);
    assert_eq!(ids(&store, 1, 1), [0, 101, 102]);
    put(&store, 1, 103, true, (Some(k2), b"", b""), max, None);
    assert_eq!(ids(&store, 1, 1), [101, 102, 103]);
    // A tokenless signal finds no tokenless victim but itself.
    let dropped = put(&store, 1, 104, true, (None, b"v", b"g"), max, None);
    assert_eq!(dropped, Put::Dropped);
    assert_eq!(ids(&store, 1, 1), [101, 102, 103]);
    assert_counters(&store);
}

#[test]
fn counters_stay_exact_across_ack_expiry_and_eviction() {
    let store = temp_store();
    let key: &[u8] = b"key";
    for id in 0..20u16 {
        let from = if id % 3 == 0 {
            (Some(key), &b""[..], &b""[..])
        } else {
            PLAIN
        };
        put(
            &store,
            (id % 2) as u8,
            id,
            id % 4 == 1,
            from,
            u64::MAX,
            Some(8 * small()),
        );
    }
    assert_counters(&store);
    store
        .ack_envelopes(
            &[0; 32],
            &[2u16.to_be_bytes().to_vec(), 3u16.to_be_bytes().to_vec()],
        )
        .unwrap();
    assert_counters(&store);
    let short = record(10);
    let expiring = NewEnvelope::new(&[5; 32], &[1], &short, false, 10);
    store.put_envelope(&expiring, u64::MAX, None).unwrap();
    assert_eq!(store.cleanup_expired(11).unwrap(), 1);
    assert_counters(&store);
    store
        .evict_envelopes(store.messages_usage().unwrap().0 / 2)
        .unwrap();
    assert_counters(&store);
    store.evict_tokenless(0, true).unwrap();
    assert_counters(&store);
}

#[test]
fn the_store_wide_share_evicts_the_largest_mailbox_and_never_tokened() {
    let store = temp_store();
    let key: &[u8] = b"key";
    put(&store, 1, 0, true, (Some(key), b"", b""), u64::MAX, None);
    for id in 1..=5 {
        put(&store, 1, id, true, PLAIN, u64::MAX, None);
    }
    for id in 0..2 {
        put(&store, 2, id, true, PLAIN, u64::MAX, None);
    }
    for id in 0..3 {
        put(&store, 3, id, false, PLAIN, u64::MAX, None);
    }
    // Signals only: mailbox 1 goes down until it is no longer the largest.
    assert_eq!(store.evict_tokenless(3 * small(), false).unwrap(), 4);
    assert_eq!(ids(&store, 1, 1), [0, 5]);
    assert_eq!(ids(&store, 2, 1).len(), 2);
    assert_eq!(ids(&store, 3, 0).len(), 3);
    assert_eq!(store.evict_tokenless(3 * small(), false).unwrap(), 0);
    assert_eq!(store.evict_tokenless(0, true).unwrap(), 6);
    assert_eq!(ids(&store, 1, 1), [0]);
    assert!(ids(&store, 2, 1).is_empty() && ids(&store, 3, 0).is_empty());
    assert_counters(&store);
}

#[test]
fn put_path_queries_use_indexes() {
    let store = temp_store();
    let db = lock(&store.messages).unwrap();
    for sql in [
        EXISTS,
        CLASS_BYTES,
        TOKENLESS_BYTES,
        TOKENLESS_ROWS,
        TOKENED_ROWS,
        DELETE,
        TOTAL,
        LARGEST,
    ] {
        let mut query = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let mut rows = query.raw_query();
        let mut plan = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            plan.push(
                row.get::<_, String>(3)
                    .unwrap()
                    .replace("SCAN CONSTANT ROW", ""),
            );
        }
        let plan = plan.join("\n");
        assert!(
            !plan.contains("SCAN") && !plan.contains("TEMP B-TREE"),
            "{sql}: {plan}"
        );
    }
}

#[test]
fn put_with_eviction_over_100k_envelopes() {
    let store = temp_store();
    // 100 mailboxes of 1000 tokenless messages of 100 bytes each.
    lock(&store.messages)
        .unwrap()
        .execute_batch(
            "WITH RECURSIVE `n`(`i`) AS (SELECT 0 UNION ALL SELECT `i` + 1 FROM `n` WHERE `i` < 99999)
             INSERT INTO `envelopes` (`mailbox`, `id`, `record`, `size`, `expires_at`, `class`, `via`,
                 `source_group`)
             SELECT CAST(printf('%032d', `i` % 100) AS BLOB), CAST(`i` AS BLOB), zeroblob(100),
                 100, 1000, 0, CAST(`i` % 3 AS BLOB), CAST(`i` % 50 AS BLOB) FROM `n`;",
        )
        .unwrap();
    let pool = Some(100_000);
    let from = (None, &b"1"[..], &b"g"[..]);
    let mailbox = format!("{:032}", 5).into_bytes();
    let new = record(100);
    let envelope = NewEnvelope {
        token_key: from.0,
        via: from.1,
        source_group: from.2,
        ..NewEnvelope::new(&mailbox, b"new", &new, false, 1000)
    };
    assert_eq!(
        store.put_envelope(&envelope, u64::MAX, pool).unwrap(),
        Put::Stored
    );
    let left: i64 = lock(&store.messages)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM `envelopes` WHERE `mailbox` = ?",
            [&mailbox],
            |row| row.get(0),
        )
        .unwrap();
    assert!(left < 1000, "{left}");
    assert_counters(&store);
}

#[test]
fn messages_tokens_step_migrates_an_old_file() {
    let dir = format!("/tmp/ai/test-bootstrap-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/messages.db");
    // A file after the `class` step.
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE `envelopes` (`seq` INTEGER PRIMARY KEY, `mailbox` BLOB NOT NULL,
                 `id` BLOB NOT NULL, `record` BLOB NOT NULL, `size` INTEGER NOT NULL,
                 `expires_at` INTEGER NOT NULL, `class` INTEGER NOT NULL DEFAULT 0,
                 UNIQUE (`mailbox`, `id`));
             INSERT INTO `envelopes` (`mailbox`, `id`, `record`, `size`, `expires_at`, `class`)
                 VALUES (x'01', x'01', x'', 10, 0, 0), (x'01', x'02', x'', 5, 0, 1),
                 (x'01', x'03', x'', 7, 0, 1);
             PRAGMA user_version = 1;",
        )
        .unwrap();
    let store = BootstrapStore::new(&dir).unwrap();
    assert_counters(&store);
    let db = lock(&store.messages).unwrap();
    let bytes: i64 = db
        .query_row(TOKENLESS_BYTES, params![&[1u8][..], 1], |row| row.get(0))
        .unwrap();
    assert_eq!(bytes, 12);
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, MESSAGE_STEPS.len() as i64);
}
