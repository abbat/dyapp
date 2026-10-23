use super::*;

const STORES: [Store; 3] = [Store::Mailbox, Store::Profile, Store::Media];

/// Keys `range` with their replica keys; a media keep lists two blob hashes.
fn batch(store: Store, range: std::ops::Range<u8>) -> Vec<(Vec<u8>, Vec<Vec<u8>>)> {
    range
        .map(|key| {
            let hashes = match store {
                Store::Media => vec![vec![key, 0], vec![key, 1]],
                _ => vec![vec![key]],
            };
            (vec![key], replica_keys(store, &hashes))
        })
        .collect()
}

/// Five of `peers` picked by the replica key, rotated by `shift`.
fn closest(peers: &[PeerId], shift: usize) -> impl Fn(&[u8]) -> Vec<PeerId> + '_ {
    move |key| {
        let start = key.iter().map(|&b| usize::from(b)).sum::<usize>() + shift;
        (0..5).map(|i| peers[(start + i) % peers.len()]).collect()
    }
}

fn peers() -> Vec<PeerId> {
    (0..8).map(|_| PeerId::random()).collect()
}

/// Pops every queued key, committing it when `commit`.
fn drain(scanner: &mut Scanner, store: Store, commit: bool) -> Vec<Vec<u8>> {
    let mut keys = Vec::new();
    while let Some((key, fingerprint)) = scanner.pop(store) {
        if commit {
            scanner.commit(store, &key, fingerprint);
        }
        keys.push(key);
    }
    keys
}

#[test]
fn unchanged_sets_queue_nothing() {
    let peers = peers();
    for store in STORES {
        let mut scanner = Scanner::default();
        scanner.scan(store, batch(store, 0..20), closest(&peers, 0), true);
        assert_eq!(drain(&mut scanner, store, true).len(), 20, "{store:?}");
        scanner.scan(store, batch(store, 0..20), closest(&peers, 0), true);
        assert!(drain(&mut scanner, store, true).is_empty(), "{store:?}");
    }
}

#[test]
fn changed_set_queues_key() {
    let peers = peers();
    for store in STORES {
        let mut scanner = Scanner::default();
        scanner.scan(store, batch(store, 0..20), closest(&peers, 0), true);
        drain(&mut scanner, store, true);
        // Key 7 alone sees other peers for one of its replica keys; key 30 is unknown.
        let shifted = closest(&peers, 1);
        let last = batch(store, 7..8)[0].1.last().unwrap().clone();
        let mixed = |key: &[u8]| {
            if key == last {
                shifted(key)
            } else {
                closest(&peers, 0)(key)
            }
        };
        let mut keys = batch(store, 0..20);
        keys.extend(batch(store, 30..31));
        scanner.scan(store, keys, mixed, true);
        assert_eq!(
            drain(&mut scanner, store, true),
            vec![vec![7], vec![30]],
            "{store:?}"
        );
    }
}

#[test]
fn cursor_resumes_and_wraps() {
    let peers = peers();
    let mut scanner = Scanner::default();
    let store = Store::Mailbox;
    scanner.scan(store, batch(store, 0..3), closest(&peers, 0), false);
    assert_eq!(scanner.cursor(store), [2]);
    scanner.scan(store, batch(store, 3..5), closest(&peers, 0), true);
    assert!(scanner.cursor(store).is_empty());
    drain(&mut scanner, store, true);

    // Key 9 is handed over, then not scanned for a whole cycle (it expired): it is forgotten
    // and queued when it is back.
    let mut with_nine = batch(store, 0..5);
    with_nine.extend(batch(store, 9..10));
    scanner.scan(store, with_nine.clone(), closest(&peers, 0), true);
    assert_eq!(drain(&mut scanner, store, true), vec![vec![9]]);
    scanner.scan(store, batch(store, 0..5), closest(&peers, 0), true);
    assert_eq!(scanner.lanes[store as usize].seen.len(), 5);
    scanner.scan(store, with_nine, closest(&peers, 0), true);
    assert_eq!(drain(&mut scanner, store, true), vec![vec![9]]);
}

#[test]
fn queue_is_capped() {
    let peers = peers();
    let store = Store::Profile;
    let mut scanner = Scanner::default();
    let keys: Vec<_> = (0..QUEUE as u32 + 10)
        .map(|n| {
            let key = n.to_be_bytes().to_vec();
            let replicas = replica_keys(store, std::slice::from_ref(&key));
            (key, replicas)
        })
        .collect();
    scanner.scan(store, keys.clone(), closest(&peers, 0), true);
    assert_eq!(scanner.lanes[store as usize].queue.len(), QUEUE);
    // Stopped before the first key left out, so no wrap.
    assert_eq!(scanner.cursor(store), (QUEUE as u32 - 1).to_be_bytes());

    // A repeated scan does not queue a key twice.
    scanner.scan(store, keys[..QUEUE].to_vec(), closest(&peers, 0), false);
    assert_eq!(scanner.lanes[store as usize].queue.len(), QUEUE);

    drain(&mut scanner, store, true);
    scanner.scan(store, keys[QUEUE..].to_vec(), closest(&peers, 0), true);
    assert_eq!(drain(&mut scanner, store, true).len(), 10);
}

#[test]
fn uncommitted_key_is_queued_again() {
    let peers = peers();
    let store = Store::Media;
    let mut scanner = Scanner::default();
    scanner.scan(store, batch(store, 0..3), closest(&peers, 0), true);
    // Key 1 failed or was cut off.
    while let Some((key, fingerprint)) = scanner.pop(store) {
        if key != [1] {
            scanner.commit(store, &key, fingerprint);
        }
    }
    scanner.scan(store, batch(store, 0..3), closest(&peers, 0), true);
    assert_eq!(drain(&mut scanner, store, true), vec![vec![1]]);
}

#[test]
fn replica_keys_per_store() {
    let one = [vec![1u8; 32]];
    assert_eq!(replica_keys(Store::Mailbox, &one).len(), 5);
    assert_eq!(replica_keys(Store::Profile, &one).len(), 5);
    assert_eq!(
        replica_keys(Store::Media, &[vec![1; 32], vec![2; 32]]).len(),
        20
    );
}
