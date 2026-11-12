//! Seeded interleavings of index writes and queries with a generation rebuild: the copy yields
//! between batches, so puts land before, during and after it, and the switch happens at a seeded
//! step. Proptest prints the failing schedule and keeps its seed.
use super::tests::{assert_digest_consistent, fill, index, profile, signed};
use super::*;
use crate::maintenance::RebuildBudget;
use dyapp_identity::Identity;
use proptest::prelude::*;
use std::collections::BTreeMap;

const NEXT: u32 = INDEX_VERSION + 1;

#[derive(Clone, Debug)]
enum Step {
    /// A publish, replica or repair of `key` at `version`, a tombstone when set.
    Put(usize, u64, bool),
    /// A search reads the active generation.
    Query,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (0usize..3, 1u64..8, prop::bool::weighted(0.2))
            .prop_map(|(key, version, deleted)| Step::Put(key, version, deleted)),
        1 => Just(Step::Query),
    ]
}

struct Run<'a> {
    index: &'a SearchIndex,
    owners: &'a [Identity; 3],
    keys: [[u8; 32]; 3],
    /// key -> (version, deleted) of the newest accepted put.
    model: BTreeMap<usize, (u64, bool)>,
    /// The highest version each key was ever seen at.
    seen: [u64; 3],
}

impl Run<'_> {
    fn apply(&mut self, step: &Step) -> std::result::Result<(), TestCaseError> {
        match *step {
            Step::Put(key, version, deleted) => {
                let p = if deleted {
                    Profile::tombstone(version, 0, 0)
                } else {
                    profile(version)
                };
                let (hash, p, record) = signed(&self.owners[key], p);
                let now = self.index.created();
                let stale = self.model.get(&key).is_some_and(|(top, _)| *top >= version);
                let put = self.index.put(&hash, &p, &record, now, false).unwrap();
                prop_assert_eq!(put, if stale { Put::Stale } else { Put::Stored });
                if !stale {
                    self.model.insert(key, (version, deleted));
                }
            }
            Step::Query => {
                let found = self.index.query(&[], 200, 10_000).unwrap().unwrap();
                for (i, key) in self.keys.iter().enumerate() {
                    let hit = found.hits.iter().find(|(k, _)| k == key).map(|(_, v)| *v);
                    let live = self
                        .model
                        .get(&i)
                        .filter(|(_, deleted)| !deleted)
                        .map(|(v, _)| *v);
                    prop_assert_eq!(hit, live, "query of key {}", i);
                }
            }
        }
        let versions = self.index.versions(&self.keys).unwrap();
        for (i, version) in versions.into_iter().enumerate() {
            prop_assert_eq!(version, self.model.get(&i).map(|(v, _)| *v));
            let version = version.unwrap_or(0);
            prop_assert!(version >= self.seen[i], "key {} went back", i);
            self.seen[i] = version;
        }
        Ok(())
    }
}

fn check(
    owners: &[Identity; 3],
    schedule: &[Step],
    switch: usize,
) -> std::result::Result<(), TestCaseError> {
    // Enough rows for several copy batches; fewer than the query limit with the three keys.
    let base = {
        let index = index();
        fill(&index, 150, index.created());
        index.path.clone()
    };
    let index = SearchIndex::open_target(&base, &crate::config::Search::default(), NEXT).unwrap();
    let keys = owners
        .each_ref()
        .map(|owner| dyapp_identity::key_hash(&owner.public_key()));
    let mut run = Run {
        index: &index,
        owners,
        keys,
        model: BTreeMap::new(),
        seen: [0; 3],
    };
    let switch = switch.min(schedule.len());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let budget = RebuildBudget::new(64);
    let (built, during) = runtime.block_on(async {
        let during = async {
            for step in &schedule[..switch] {
                run.apply(step)?;
                tokio::task::yield_now().await;
            }
            Ok::<_, TestCaseError>(())
        };
        tokio::join!(index.build(NEXT, &budget, 0), during)
    });
    during?;
    index.activate(built.unwrap(), Replay::All).unwrap();
    prop_assert_eq!(index.index_version(), NEXT);
    for step in &schedule[switch..] {
        run.apply(step)?;
    }
    run.apply(&Step::Query)?;
    assert_digest_consistent(&index);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "/tmp/ai/dyapp-bootstrap-search-regressions.txt"
            )
        )),
        ..ProptestConfig::default()
    })]

    #[test]
    fn index_writes_queries_and_a_rebuild_interleave(
        schedule in prop::collection::vec(step(), 1..40),
        switch in 0usize..40,
    ) {
        thread_local! {
            static OWNERS: [Identity; 3] =
                [Identity::generate(), Identity::generate(), Identity::generate()];
        }
        OWNERS.with(|owners| check(owners, &schedule, switch))?;
    }
}
