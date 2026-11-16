//! Seeded interleavings of several writers on one store. Every store call holds its connection
//! lock, so a schedule of whole calls covers the reachable orders; proptest prints the failing
//! schedule and keeps its seed.

use super::*;
use dyapp_p2p_net::proto::KeyPackageRecord;
use proptest::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

fn config(name: &str) -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(Box::leak(
                format!("build/dyapp-bootstrap-{name}-regressions.txt").into_boxed_str(),
            )),
        )),
        ..ProptestConfig::default()
    }
}

#[derive(Clone, Debug)]
enum Mail {
    /// A sender or replica puts message or signal `id` with an explicit expiry.
    Put(u8, bool, i64),
    /// The device fetches the next page of its pass.
    Fetch(u32),
    /// The device acks the page it fetched last.
    Ack,
    /// Another device or a replica ack removes `id`.
    OtherAck(u8),
    /// Node time advances and expires old envelopes.
    Advance(i64),
    /// The node closes and opens the same stores.
    Restart,
    /// SQLite rejects an insert and the model stays unchanged.
    FailPut,
}

fn mail() -> impl Strategy<Value = Mail> {
    prop_oneof![
        3 => (0u8..10, any::<bool>(), 1i64..20)
            .prop_map(|(id, signal, expires)| Mail::Put(id, signal, expires)),
        2 => (1u32..4).prop_map(Mail::Fetch),
        1 => Just(Mail::Ack),
        1 => (0u8..10).prop_map(Mail::OtherAck),
        1 => (0i64..20).prop_map(Mail::Advance),
        1 => Just(Mail::Restart),
        1 => Just(Mail::FailPut),
    ]
}

fn ids(records: &[SignedRecord]) -> Vec<u8> {
    records.iter().map(|record| record.payload[0]).collect()
}

const MAILBOX: [u8; 32] = [5; 32];

/// The device's fetch pass over one mailbox.
#[derive(Default)]
struct Pass {
    stored: HashMap<u8, i64>,
    cursor: Option<(i64, i64)>,
    page: Vec<u8>,
    /// Envelopes stored since the pass began and the ids the pass returned.
    whole: HashSet<u8>,
    seen: Vec<u8>,
}

impl Pass {
    fn fetch(
        &mut self,
        store: &BootstrapStore,
        limit: u32,
    ) -> std::result::Result<(), TestCaseError> {
        if self.cursor.is_none() {
            self.whole = self.stored.keys().copied().collect();
            self.seen.clear();
        }
        let (records, more, next) = store
            .fetch_envelopes_after(&MAILBOX, self.cursor.take(), limit, u64::MAX)
            .unwrap();
        self.page = ids(&records);
        for id in &self.page {
            prop_assert!(
                self.stored.contains_key(id),
                "acked envelope {id} came back"
            );
        }
        self.seen.extend_from_slice(&self.page);
        if more {
            self.cursor = next;
        } else {
            for id in &self.whole {
                prop_assert!(self.seen.contains(id), "envelope {id} skipped by the pass");
            }
        }
        Ok(())
    }

    fn ack(&mut self, store: &BootstrapStore, acked: &[Vec<u8>]) {
        store.ack_envelopes(&MAILBOX, acked).unwrap();
        for id in acked {
            self.stored.remove(&id[0]);
            self.whole.remove(&id[0]);
        }
    }
}

fn reopen(store: BootstrapStore) -> BootstrapStore {
    let dir = {
        let db = lock(&store.profiles).unwrap();
        PathBuf::from(db.path().unwrap())
            .parent()
            .unwrap()
            .to_path_buf()
    };
    drop(store);
    BootstrapStore::new(dir.to_str().unwrap()).unwrap()
}

fn check_mailbox_bytes(store: &BootstrapStore) -> std::result::Result<(), TestCaseError> {
    let db = lock(&store.messages).unwrap();
    for class in [0, 1] {
        let stored: i64 = db.query_row(
            "SELECT COALESCE(SUM(`size`), 0) FROM `envelopes`
             WHERE `mailbox` = ? AND `class` = ? AND `token_key` IS NULL",
            params![&MAILBOX[..], class],
            |row| row.get(0),
        )?;
        let charged: i64 = db.query_row(
            "SELECT COALESCE(MAX(`bytes`), 0) FROM `tokenless_mailbox`
             WHERE `mailbox` = ? AND `class` = ?",
            params![&MAILBOX[..], class],
            |row| row.get(0),
        )?;
        prop_assert_eq!(charged, stored, "class {} byte counter", class);
    }
    Ok(())
}

/// Puts, fetch pages and acks from several writers. An acked envelope never comes back unless
/// it is put again, an envelope stored for a whole pass is returned in that pass, and a final
/// pass returns every unacked envelope once.
fn fail_mailbox_put(store: &BootstrapStore) -> std::result::Result<(), TestCaseError> {
    lock(&store.messages).unwrap().execute_batch(
        "CREATE TRIGGER `fail_model_put` BEFORE INSERT ON `envelopes`
         BEGIN SELECT RAISE(ABORT, 'injected model failure'); END;",
    )?;
    let record = SignedRecord {
        payload: vec![u8::MAX],
        ..SignedRecord::default()
    };
    prop_assert!(store
        .put_plain(&MAILBOX, &[u8::MAX], &record, false, i64::MAX, u64::MAX)
        .is_err());
    lock(&store.messages)
        .unwrap()
        .execute_batch("DROP TRIGGER `fail_model_put`")?;
    Ok(())
}

fn check_mailbox(schedule: &[Mail]) -> std::result::Result<(), TestCaseError> {
    let mut store = temp_store();
    let mut pass = Pass::default();
    for step in schedule {
        match *step {
            Mail::Put(id, signal, expires) => {
                let record = SignedRecord {
                    payload: vec![id],
                    ..SignedRecord::default()
                };
                prop_assert!(store
                    .put_plain(&MAILBOX, &[id], &record, signal, expires, u64::MAX)
                    .unwrap());
                pass.stored.entry(id).or_insert(expires);
            }
            Mail::Fetch(limit) => pass.fetch(&store, limit)?,
            Mail::Ack => {
                let acked: Vec<Vec<u8>> = pass.page.drain(..).map(|id| vec![id]).collect();
                pass.ack(&store, &acked);
            }
            Mail::OtherAck(id) => pass.ack(&store, &[vec![id]]),
            Mail::Advance(now) => {
                store.cleanup_expired(now).unwrap();
                pass.stored.retain(|_, expires| *expires > now);
                pass.whole.retain(|id| pass.stored.contains_key(id));
            }
            Mail::Restart => store = reopen(store),
            Mail::FailPut => fail_mailbox_put(&store)?,
        }
        check_mailbox_bytes(&store)?;
    }
    let (records, more, _) = store
        .fetch_envelopes_after(&MAILBOX, None, 100, u64::MAX)
        .unwrap();
    let mut last = ids(&records);
    prop_assert!(!more);
    last.sort_unstable();
    let mut expected: Vec<u8> = pass.stored.into_keys().collect();
    expected.sort_unstable();
    prop_assert_eq!(last, expected);
    Ok(())
}

#[derive(Clone, Debug)]
struct Publish {
    owner: usize,
    version: u64,
    deleted: bool,
    last_seen: i64,
}

#[derive(Clone, Debug)]
enum ProfileStep {
    Apply(Publish),
    Restart,
}

fn profile_step() -> impl Strategy<Value = ProfileStep> {
    prop_oneof![
        8 => (0usize..2, 1u64..8, prop::bool::weighted(0.15), 0i64..100).prop_map(
            |(owner, version, deleted, last_seen)| ProfileStep::Apply(Publish {
                owner,
                version,
                deleted,
                last_seen,
            })
        ),
        1 => Just(ProfileStep::Restart),
    ]
}

/// The owner's publish, replica apply and repair all land as `put_profile_seen` in any order:
/// the newest revision wins, the stored version never drops, a tombstone is final and the last
/// seen time only grows.
fn published_profile(store: &BootstrapStore, owner: &Identity, step: &Publish) -> SignedRecord {
    if step.deleted {
        let source = store.get_profile(&owner.peer_id()).unwrap();
        Profile {
            previous: Some(dyapp_profile::ProfileCondition::observed(source.as_ref()).unwrap()),
            ..Profile::tombstone(step.version, 0, 0)
        }
        .sign(owner)
    } else {
        Profile {
            version: step.version,
            ..Profile::default()
        }
        .sign(owner)
    }
}

fn check_profiles(
    owners: &[Identity; 2],
    schedule: &[ProfileStep],
) -> std::result::Result<(), TestCaseError> {
    let mut store = temp_store();
    let mut model: [Option<(u64, bool, i64)>; 2] = [None; 2];
    for transition in schedule {
        match transition {
            ProfileStep::Restart => store = reopen(store),
            ProfileStep::Apply(step) => {
                let owner = &owners[step.owner];
                let record = published_profile(&store, owner, step);
                let slot = &mut model[step.owner];
                let accept =
                    slot.is_none_or(|(version, deleted, _)| step.version > version && !deleted);
                let result = store.put_profile_seen(&record, step.last_seen);
                prop_assert_eq!(result.is_ok(), accept, "{:?} over {:?}", step, slot);
                if accept {
                    let seen = slot.map_or(step.last_seen, |(.., seen)| seen.max(step.last_seen));
                    *slot = Some((step.version, step.deleted, seen));
                }
            }
        }
        for (owner, expected) in owners.iter().zip(&model) {
            let peer = owner.peer_id();
            let stored = store
                .get_profile(&peer)
                .unwrap()
                .map(|record| dyapp_profile::verify(&record).unwrap().profile);
            prop_assert_eq!(
                stored.map(|profile| (profile.version, profile.deleted)),
                expected.map(|(version, deleted, _)| (version, deleted))
            );
            prop_assert_eq!(
                store.profile_last_seen(&peer).unwrap(),
                expected.map(|(.., seen)| seen)
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
enum Stock {
    /// The owner uploads ordinary package `n` (a retry when it was uploaded before).
    Ordinary(u8),
    /// The owner uploads last-resort revision `n`.
    LastResort(u8),
    /// A sender allocates.
    Allocate,
    /// The node closes and opens the same stores.
    Restart,
}

fn stock() -> impl Strategy<Value = Stock> {
    prop_oneof![
        3 => (0u8..30).prop_map(Stock::Ordinary),
        1 => (1u8..6).prop_map(Stock::LastResort),
        3 => Just(Stock::Allocate),
        1 => Just(Stock::Restart),
    ]
}

fn stocked(last_resort: bool, n: u8) -> (KeyPackageRecord, Vec<u8>, SignedRecord) {
    let hash = vec![u8::from(last_resort), n];
    let package = KeyPackageRecord {
        owner: vec![1; 32],
        mailbox: vec![2; 32],
        last_resort,
        revision: u32::from(n),
        expires: u64::MAX,
        ..KeyPackageRecord::default()
    };
    let record = SignedRecord {
        payload: hash.clone(),
        ..SignedRecord::default()
    };
    (package, hash, record)
}

/// Uploads, retries, last-resort rotation and allocations in any order: each ordinary package
/// is handed out at most once, a consumed one is never revived by a retry, and the fallback is
/// the newest last-resort revision.
fn check_keypackages(schedule: &[Stock]) -> std::result::Result<(), TestCaseError> {
    let mut store = temp_store();
    // hash -> consumed, ordinary and last resort apart.
    let mut ordinary = BTreeMap::<u8, bool>::new();
    let mut last_resort = BTreeMap::<u8, bool>::new();
    let mut handed = HashSet::new();
    for step in schedule {
        match *step {
            Stock::Ordinary(n) => {
                let (package, hash, record) = stocked(false, n);
                let live = ordinary.values().filter(|consumed| !**consumed).count();
                let accept = ordinary.contains_key(&n) || live < 20;
                prop_assert_eq!(
                    store.put_keypackage(&package, &hash, 0, &record).unwrap(),
                    accept
                );
                if accept {
                    ordinary.entry(n).or_insert(false);
                }
            }
            Stock::LastResort(n) => {
                let (package, hash, record) = stocked(true, n);
                let known = last_resort.contains_key(&n);
                let newer = last_resort.keys().next_back().is_none_or(|top| n > *top);
                prop_assert_eq!(
                    store.put_keypackage(&package, &hash, 0, &record).unwrap(),
                    known || newer
                );
                if !known && newer {
                    for consumed in last_resort.values_mut() {
                        *consumed = true;
                    }
                    last_resort.insert(n, false);
                }
            }
            Stock::Allocate => {
                let got = allocate(&store);
                prop_assert_eq!(got, expected(&mut ordinary, &last_resort, &mut handed)?);
            }
            Stock::Restart => store = reopen(store),
        }
    }
    Ok(())
}

fn allocate(store: &BootstrapStore) -> Option<Vec<u8>> {
    store
        .allocate_keypackage(&[1; 32], &[2; 32], 0, 0)
        .unwrap()
        .map(|record| record.payload)
}

/// The model's allocation: the oldest unconsumed ordinary package, else the live fallback.
fn expected(
    ordinary: &mut BTreeMap<u8, bool>,
    last_resort: &BTreeMap<u8, bool>,
    handed: &mut HashSet<u8>,
) -> std::result::Result<Option<Vec<u8>>, TestCaseError> {
    if let Some((n, consumed)) = ordinary.iter_mut().find(|(_, consumed)| !**consumed) {
        *consumed = true;
        prop_assert!(handed.insert(*n), "ordinary {n} handed out twice");
        return Ok(Some(vec![0, *n]));
    }
    Ok(last_resort
        .iter()
        .find(|(_, consumed)| !**consumed)
        .map(|(n, _)| vec![1, *n]))
}

fn upload(store: &BootstrapStore, last_resort: bool, n: u8) {
    let (package, hash, record) = stocked(last_resort, n);
    assert!(store.put_keypackage(&package, &hash, 0, &record).unwrap());
}

proptest! {
    #![proptest_config(config("mailbox"))]
    #[test]
    fn mailbox_writers_interleave_without_lost_or_revived_envelopes(
        schedule in prop::collection::vec(mail(), 1..60),
    ) {
        check_mailbox(&schedule)?;
    }
}

proptest! {
    #![proptest_config(config("profiles"))]
    #[test]
    fn profile_publish_replica_repair_and_tombstone_interleave(
        schedule in prop::collection::vec(profile_step(), 1..30),
    ) {
        thread_local! {
            static OWNERS: [Identity; 2] = [Identity::generate(), Identity::generate()];
        }
        OWNERS.with(|owners| check_profiles(owners, &schedule))?;
    }
}

proptest! {
    #![proptest_config(config("keypackages"))]
    #[test]
    fn keypackage_upload_retry_rotation_and_allocation_interleave(
        schedule in prop::collection::vec(stock(), 1..80),
    ) {
        check_keypackages(&schedule)?;
    }
}

#[test]
fn concurrent_allocations_hand_out_each_ordinary_package_once() {
    let store = temp_store();
    for n in 0..20 {
        upload(&store, false, n);
    }
    upload(&store, true, 1);
    let handed: Vec<Vec<u8>> = std::thread::scope(|scope| {
        let store = &store;
        // The owner retries every upload and rotates the fallback while senders allocate.
        scope.spawn(move || {
            for n in 0..20 {
                upload(store, false, n);
            }
            upload(store, true, 2);
        });
        let senders: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(move || (0..5).filter_map(|_| allocate(store)).collect::<Vec<_>>())
            })
            .collect();
        senders
            .into_iter()
            .flat_map(|sender| sender.join().unwrap())
            .collect()
    });
    let mut counts = HashMap::<Vec<u8>, usize>::new();
    for payload in &handed {
        *counts.entry(payload.clone()).or_default() += 1;
    }
    let ordinary: Vec<_> = counts
        .iter()
        .filter(|(payload, _)| payload[0] == 0)
        .collect();
    assert_eq!(ordinary.len(), 20);
    assert!(ordinary.iter().all(|(_, count)| **count == 1));
    assert_eq!(handed.len(), 40);
    assert_eq!(allocate(&store), Some(vec![1, 2]));
}
