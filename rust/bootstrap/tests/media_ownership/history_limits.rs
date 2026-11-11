//! Exact encoding boundaries and completion-driven competing ownership writers.
use super::*;
use std::sync::mpsc;

fn padded(identity: &Identity, payload: &[u8], padding: usize) -> SignedRecord {
    let mut bytes = payload.to_vec();
    prost::encoding::bytes::encode(127, &vec![0u8; padding], &mut bytes);
    identity.sign(Domain::MediaConditionalKeep, bytes)
}

#[test]
fn a_maximum_sized_accepted_record_remains_retrievable_with_its_framing() {
    let fixture = Fixture::new();
    let payload = fixture
        .write(&KeepSnapshot::default(), Vec::new())
        .encode_to_vec();
    let probe = padded(&fixture.owner, &payload, 1 << 20);
    let overhead = probe.encoded_len() - (1 << 20);
    let record = padded(&fixture.owner, &payload, (1 << 20) - overhead);
    assert_eq!(record.encoded_len(), 1 << 20);
    let response = fixture.call(
        &fixture.peer,
        media_request::Request::ConditionalKeep(record.clone()),
    );
    assert_eq!(response.status, i32::from(Status::Ok));
    let limit = MediaStore::keep_history_page_bytes();
    assert_eq!(limit, (1 << 20) + 4);
    assert!(fixture
        .media()
        .keep_history(&fixture.owner.peer_id(), 0, 1, limit - 1)
        .is_err());
    let page = fixture
        .media()
        .keep_history(&fixture.owner.peer_id(), 0, 1, limit)
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(
        sha256(&page[0].encode_to_vec()),
        sha256(&record.encode_to_vec())
    );
}

#[test]
fn pagination_caps_record_count_and_resumes_without_a_gap() {
    let fixture = Fixture::new();
    fixture.initial();
    for revision in 2u64..=258 {
        let current = fixture
            .media()
            .keep_snapshot(&fixture.owner.peer_id())
            .unwrap();
        let intent = fixture.write(&current, vec![sha256(&revision.to_be_bytes()).to_vec()]);
        assert_eq!(fixture.apply(&intent).status, i32::from(Status::Ok));
    }
    let owner = fixture.owner.peer_id();
    let first = fixture
        .media()
        .keep_history(&owner, 0, 258, 1 << 20)
        .unwrap();
    assert_eq!(first.len(), 256);
    let last = proto::MediaKeepIntent::decode(first.last().unwrap().payload.as_slice()).unwrap();
    assert_eq!(last.version, 256);
    let next = fixture
        .media()
        .keep_history(&owner, last.version, 258, 1 << 20)
        .unwrap();
    assert_eq!(next.len(), 2);
    let target = proto::MediaKeepIntent::decode(next.last().unwrap().payload.as_slice()).unwrap();
    assert_eq!(target.version, 258);
}

type Completion = (SignedRecord, i32);

fn winner(results: &[Completion]) -> &SignedRecord {
    let winners: Vec<_> = results
        .iter()
        .filter_map(|(record, status)| (*status == i32::from(Status::Ok)).then_some(record))
        .collect();
    assert_eq!(winners.len(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|(_, status)| *status == i32::from(Status::Stale))
            .count(),
        1
    );
    winners[0]
}

fn competitor(
    fixture: &Fixture,
    record: SignedRecord,
    ready: mpsc::Sender<()>,
    done: mpsc::Sender<Completion>,
) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let service = Arc::clone(&fixture.service);
    let peer = Peer {
        id: fixture.peer.id.clone(),
        group: fixture.peer.group.clone(),
        coarse: fixture.peer.coarse.clone(),
    };
    let (release, wait) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        ready.send(()).unwrap();
        if wait.recv().is_err() {
            return;
        }
        let response = service
            .media(
                &peer,
                proto::MediaRequest {
                    request: Some(media_request::Request::ConditionalKeep(record.clone())),
                },
            )
            .unwrap();
        done.send((record, response.status)).unwrap();
    });
    (release, worker)
}

#[test]
fn competing_writes_record_only_the_winner_in_atomic_history() {
    let fixture = Fixture::new();
    let original = fixture.initial();
    let before = fixture
        .media()
        .keep_snapshot(&fixture.owner.peer_id())
        .unwrap();
    let (ready, readiness) = mpsc::channel();
    let (done, completion) = mpsc::channel();
    let mut workers = Vec::new();
    for hash in [vec![1; 32], vec![2; 32]] {
        let intent = fixture.write(&before, vec![hash]);
        workers.push(competitor(
            &fixture,
            fixture.signed(&intent),
            ready.clone(),
            done.clone(),
        ));
    }
    for _ in &workers {
        readiness
            .recv_timeout(Duration::from_secs(5))
            .expect("writer readiness");
    }
    for (release, _) in &workers {
        release.send(()).unwrap();
    }
    let results: Vec<_> = (0..workers.len())
        .map(|_| {
            completion
                .recv_timeout(Duration::from_secs(5))
                .expect("writer completion")
        })
        .collect();
    for (_, worker) in workers {
        worker.join().unwrap();
    }
    let winner = winner(&results);
    assert_eq!(
        fixture
            .media()
            .keep_history(&fixture.owner.peer_id(), 0, 2, 1 << 20)
            .unwrap(),
        vec![original, winner.clone()]
    );
}

#[test]
fn reverse_pages_start_at_the_frozen_target_and_resume_without_mutating_ownership() {
    let fixture = Fixture::new();
    fixture.initial();
    for revision in 2u64..=5 {
        let current = fixture
            .media()
            .keep_snapshot(&fixture.owner.peer_id())
            .unwrap();
        let intent = fixture.write(&current, vec![sha256(&revision.to_be_bytes()).to_vec()]);
        assert_eq!(fixture.apply(&intent).status, i32::from(Status::Ok));
    }
    let owner = fixture.owner.peer_id();
    let before = fixture.media().keep_snapshot(&owner).unwrap();
    let used = fixture.media().usage().unwrap().0;
    let all = fixture
        .media()
        .keep_history_reverse(&owner, 4, 1 << 20)
        .unwrap();
    let versions: Vec<_> = all
        .iter()
        .map(|record| {
            proto::MediaKeepIntent::decode(record.payload.as_slice())
                .unwrap()
                .version
        })
        .collect();
    assert_eq!(versions, vec![4, 3, 2, 1]);
    let mut encoded = Vec::new();
    prost::encoding::message::encode(1, &all[0], &mut encoded);
    let page = fixture
        .media()
        .keep_history_reverse(&owner, 4, encoded.len())
        .unwrap();
    assert_eq!(page, vec![all[0].clone()]);
    let next = fixture
        .media()
        .keep_history_reverse(&owner, 3, 1 << 20)
        .unwrap();
    assert_eq!(next, all[1..]);
    assert!(fixture
        .media()
        .keep_history_reverse(&owner, 4, encoded.len() - 1)
        .is_err());
    assert_eq!(fixture.media().keep_snapshot(&owner).unwrap(), before);
    assert_eq!(fixture.media().usage().unwrap().0, used);
}
