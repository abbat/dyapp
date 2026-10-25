//! Profile search across several search nodes. Each node is untrusted and holds only a sample of
//! the index, so the client asks all of them, fetches what its cache lacks and re-checks every
//! record with [`search_results::check`].

use crate::profiles::Profiles;
use crate::store::{self, Store};
use crate::{Error, Node};
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto::{Condition, Status};
use dyapp_p2p_net::search_results;
use dyapp_profile::VerifiedProfile;

/// Keys one get request may carry.
const GET_KEYS: usize = 50;

/// Profiles matching every condition, from up to `limit` hits (0 means 50) of each node in
/// `nodes`, with at least `min_pow_bits` of profile proof of work: one per owner at the highest
/// version seen, in no set order. A hit whose version `cache` already holds is taken from the
/// cache, not fetched, and its proof of work is not hashed again. A node that fails is skipped,
/// so the result may miss profiles, as each node's sample may anyway.
pub async fn search(
    nodes: &mut [Node],
    conditions: &[Condition],
    limit: u32,
    min_pow_bits: u32,
    cache: &Profiles,
) -> Vec<VerifiedProfile> {
    let (mut replies, mut cached) = (Vec::new(), Vec::new());
    for node in nodes {
        let Ok(reply) = node.search_query(conditions.to_vec(), limit).await else {
            continue;
        };
        let mut keys = Vec::new();
        for hit in reply.hits {
            let peer_id: String = hit.key.iter().map(|byte| format!("{byte:02x}")).collect();
            match cache.get(&peer_id) {
                Some(held) if held.profile.version >= hit.version => cached.push(held),
                _ => keys.push(hit.key),
            }
        }
        replies.push(fetch(node, keys).await);
    }
    let mut found = search_results::check(conditions, min_pow_bits, &replies);
    // A node may serve an older version than the cache holds.
    for profile in &mut found {
        match cache.get(&profile.peer_id) {
            Some(held) if held.profile.version > profile.profile.version => *profile = held.clone(),
            _ => {}
        }
    }
    for held in cached {
        if !found.iter().any(|profile| profile.peer_id == held.peer_id) {
            found.push(held.clone());
        }
    }
    found.retain(|profile| search_results::passes(conditions, min_pow_bits, &profile.profile));
    found
}

/// Sends the profile [`Store::hold_search_profile`] holds to every node in `nodes` (search nodes
/// from [`Node::providers`] with [`dyapp_p2p_net::SEARCH_KEY`], two are enough since they gossip
/// it on) once its window has ended at Unix second `now`. Returns whether a node took it; else
/// it stays held, for the `RATE_LIMITED` wait if a node gave one, and is sent on a later call.
pub async fn publish_due(
    store: &Store,
    nodes: &mut [Node],
    now: u64,
) -> Result<bool, store::Error> {
    let Some(record) = store.due_search_profile(now)? else {
        return Ok(false);
    };
    let (mut taken, mut wait) = (false, None);
    for node in nodes {
        match node.search_publish(record.clone()).await {
            // A node that holds this version already took it.
            Ok(()) | Err(Error::Status(Status::Stale)) => taken = true,
            Err(Error::RateLimited(after)) => wait = wait.max(Some(after)),
            Err(_) => {}
        }
    }
    if taken {
        store.search_profile_sent(&record, now)?;
    } else if let Some(after) = wait {
        store.search_profile_rate_limited(now, after)?;
    }
    Ok(taken)
}

// The records of `keys` that `node` returns, asking again for those a cut reply left out.
async fn fetch(node: &mut Node, keys: Vec<Vec<u8>>) -> Vec<SignedRecord> {
    let mut records = Vec::new();
    for chunk in keys.chunks(GET_KEYS) {
        let mut wanted = chunk.to_vec();
        while !wanted.is_empty() {
            let Ok(reply) = node.search_get(wanted.clone()).await else {
                break;
            };
            let asked = wanted.len();
            for record in reply.records {
                let key = dyapp_identity::sha256(&record.public_key);
                if let Some(index) = wanted.iter().position(|wanted| *wanted == key) {
                    wanted.swap_remove(index);
                    records.push(record);
                }
            }
            if !reply.partial || wanted.len() == asked {
                break;
            }
        }
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_p2p_net::proto::{search_request::Request, Op, SearchHit, SearchResponse, Status};
    use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
    use dyapp_profile::Profile;
    use libp2p::futures::StreamExt;
    use libp2p::identity::Keypair;
    use libp2p::request_response::{Event, Message};
    use libp2p::swarm::SwarmEvent;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn record(identity: &Identity, version: u64, age: u32) -> SignedRecord {
        Profile {
            version,
            age,
            ..Profile::default()
        }
        .sign(identity)
    }

    fn key(identity: &Identity) -> Vec<u8> {
        dyapp_identity::key_hash(&identity.public_key()).to_vec()
    }

    fn hit(key: Vec<u8>, version: u64) -> SearchHit {
        SearchHit { key, version }
    }

    fn ok(hits: Vec<SearchHit>, records: Vec<SignedRecord>, partial: bool) -> SearchResponse {
        SearchResponse {
            status: Status::Ok.into(),
            hits,
            records,
            partial,
            ..SearchResponse::default()
        }
    }

    type Requests = Arc<Mutex<Vec<Request>>>;

    // A search node that answers each request with `answer` and keeps what it was asked.
    async fn fake(
        mut answer: impl FnMut(&Request) -> SearchResponse + Send + 'static,
    ) -> (Node, Requests) {
        let options = SwarmOptions {
            search: true,
            ..SwarmOptions::default()
        };
        let mut server = build_configured_swarm(Keypair::generate_ed25519(), options).unwrap();
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        let requests = Requests::default();
        let seen = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                if let SwarmEvent::Behaviour(BehaviourEvent::Search(Event::Message {
                    message:
                        Message::Request {
                            request, channel, ..
                        },
                    ..
                })) = server.select_next_some().await
                {
                    let request = request.request.unwrap();
                    let response = answer(&request);
                    seen.lock().unwrap().push(request);
                    let _ = server
                        .behaviour_mut()
                        .search
                        .send_response(channel, response);
                }
            }
        });
        let node = Node::connect(address, Arc::new(Protocols::public()))
            .await
            .unwrap();
        (node, requests)
    }

    fn gets(requests: &Requests) -> Vec<Vec<Vec<u8>>> {
        let requests = requests.lock().unwrap();
        let gets = requests.iter().filter_map(|request| match request {
            Request::Get(get) => Some(get.keys.clone()),
            _ => None,
        });
        gets.collect()
    }

    #[tokio::test]
    async fn merges_nodes_skips_cached_hits_and_resends_cut_gets() {
        let [a, b, c, d, e, f, g, h] = [(); 8].map(|()| Identity::generate());
        let mut cache = Profiles::default();
        let held = [(&a, 1, 30), (&b, 1, 30), (&e, 5, 60), (&f, 5, 31)];
        for (identity, version, age) in held {
            let held = record(identity, version, age);
            cache.accept(dyapp_profile::verify(&held).unwrap()).unwrap();
        }
        // a fits and is newer than cached, b is cached, c misses the age, f is served older than
        // cached, g never comes.
        let hits = vec![
            hit(key(&a), 2),
            hit(key(&b), 1),
            hit(key(&c), 3),
            hit(key(&f), 6),
            hit(key(&g), 1),
        ];
        let mut first = vec![
            ok(hits, Vec::new(), false),
            ok(Vec::new(), vec![record(&a, 2, 30)], true),
            ok(Vec::new(), vec![record(&c, 3, 50), record(&f, 4, 30)], true),
            ok(Vec::new(), Vec::new(), true),
        ]
        .into_iter();
        let (one, one_asked) = fake(move |_| first.next().unwrap()).await;
        // 51 uncached hits: two gets. d was not asked for; e is cached newer and no longer fits.
        let mut hits: Vec<_> = (0..51u8).map(|n| hit(vec![n; 32], 1)).collect();
        // A whole reply that misses keys is not asked again.
        hits[0] = hit(key(&h), 1);
        hits.insert(0, hit(key(&e), 4));
        let mut second = vec![
            ok(hits, Vec::new(), false),
            ok(
                Vec::new(),
                vec![record(&d, 1, 30), record(&e, 4, 30), record(&h, 1, 30)],
                false,
            ),
            ok(Vec::new(), Vec::new(), false),
        ]
        .into_iter();
        let (two, two_asked) = fake(move |_| second.next().unwrap()).await;
        let (down, _) = fake(|_| SearchResponse {
            status: Status::Denied.into(),
            ..SearchResponse::default()
        })
        .await;
        let conditions = [Condition {
            field: 3,
            op: Op::Range.into(),
            ints: vec![25, 35],
            strings: Vec::new(),
        }];
        let mut nodes = [one, two, down];
        let search = search(&mut nodes, &conditions, 0, 0, &cache);
        let found = tokio::time::timeout(Duration::from_secs(10), search)
            .await
            .unwrap();
        let mut found: Vec<_> = found
            .iter()
            .map(|p| (p.peer_id.clone(), p.profile.version))
            .collect();
        found.sort();
        let mut expected = vec![
            (a.peer_id(), 2),
            (b.peer_id(), 1),
            (f.peer_id(), 5),
            (h.peer_id(), 1),
        ];
        expected.sort();
        assert_eq!(found, expected);
        let one_gets = gets(&one_asked);
        assert_eq!(one_gets.len(), 3);
        assert_eq!(one_gets[0], [key(&a), key(&c), key(&f), key(&g)]);
        assert_eq!(one_gets[1].len(), 3);
        assert_eq!(one_gets[2], [key(&g)]);
        let two_gets = gets(&two_asked);
        assert_eq!(two_gets.iter().map(Vec::len).collect::<Vec<_>>(), [50, 1]);
        assert!(!two_gets.concat().contains(&key(&e)));
    }

    async fn within(
        publish: impl std::future::Future<Output = Result<bool, store::Error>>,
    ) -> bool {
        tokio::time::timeout(Duration::from_secs(10), publish)
            .await
            .unwrap()
            .unwrap()
    }

    fn status(status: Status, retry_after: u64) -> SearchResponse {
        SearchResponse {
            status: status.into(),
            retry_after,
            ..SearchResponse::default()
        }
    }

    #[tokio::test]
    async fn publishes_the_held_profile_once_due_and_waits_out_rate_limits() {
        let alice = Identity::generate();
        let store = Store::open(&crate::store::tests::temp_db_path(), &alice).unwrap();
        let first = record(&alice, 1, 30);
        let busy = [
            (Status::RateLimited, 60),
            (Status::Denied, 0),
            (Status::Ok, 0),
        ];
        let mut busy = busy.map(|(s, after)| status(s, after)).into_iter();
        let (busy, busy_asked) = fake(move |_| busy.next().unwrap()).await;
        let slow = [
            (Status::RateLimited, 120),
            (Status::Stale, 0),
            (Status::Denied, 0),
        ];
        let mut slow = slow.map(|(s, after)| status(s, after)).into_iter();
        let (slow, _) = fake(move |_| slow.next().unwrap()).await;
        let mut nodes = [busy, slow];
        // Nothing held: no node is asked.
        assert!(!within(publish_due(&store, &mut nodes, 1000)).await);
        store.hold_search_profile(&first).unwrap();
        // Both nodes busy: held for the longer wait.
        assert!(!within(publish_due(&store, &mut nodes, 1000)).await);
        assert_eq!(store.due_search_profile(1119).unwrap(), None);
        assert_eq!(store.due_search_profile(1120).unwrap(), Some(first.clone()));
        // One fails and the other has it already: the window starts.
        assert!(within(publish_due(&store, &mut nodes, 1120)).await);
        assert_eq!(store.search_profile_minutes_left(1120).unwrap(), None);
        let second = record(&alice, 2, 30);
        store.hold_search_profile(&second).unwrap();
        assert_eq!(store.due_search_profile(1719).unwrap(), None);
        // One takes it.
        assert!(within(publish_due(&store, &mut nodes, 1720)).await);
        assert_eq!(store.due_search_profile(9999).unwrap(), None);
        let asked = busy_asked.lock().unwrap();
        let sent: Vec<_> = asked
            .iter()
            .map(|r| match r {
                Request::Publish(sent) => sent.clone(),
                _ => panic!("not a publish"),
            })
            .collect();
        assert_eq!(sent, [first.clone(), first, second]);
    }
}
