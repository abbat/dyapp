//! Search over the wire: spreading between nodes and busy nodes.
use super::*;

/// Starts a store and search node; `seed` joins it to an earlier one.
async fn start_search(seed: Option<&Multiaddr>) -> Multiaddr {
    let seed = seed.map(|addr| {
        let port = addr.iter().find_map(|p| match p {
            libp2p::multiaddr::Protocol::Tcp(port) => Some(port),
            _ => None,
        });
        format!("127.0.0.1:{}", port.unwrap())
    });
    start_config(|c| {
        c.roles = vec![Role::Store, Role::Search];
        c.seeds = seed.into_iter().collect();
        c.limits.requests_per_second = 1_000;
        c.limits.bytes_per_second = 1 << 20;
        c.search.min_profile_pow_bits = 2;
    })
    .await
}

fn age_query(age: i64) -> proto::search_request::Request {
    proto::search_request::Request::Query(proto::SearchQuery {
        conditions: vec![proto::Condition {
            field: 3,
            op: proto::Op::Eq.into(),
            ints: vec![age],
            strings: vec![],
        }],
        limit: 0,
        puzzle: None,
    })
}

#[tokio::test]
async fn search_profile_spreads_between_nodes() {
    use proto::search_request::Request;
    dyapp_p2p_net::set_id_pow_bits(0);
    let a = start_search(None).await;
    let b = start_search(Some(&a)).await;
    let mut to_a = Client::connect(&a).await;
    let mut to_b = Client::connect(&b).await;
    let mut published = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    // Gossip only reaches B once the two nodes have exchanged subscriptions, so keep publishing.
    let hit = loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no profile reached B"
        );
        let age = 20 + published.len() as i64;
        let identity = Identity::generate();
        let pow_nonce = dyapp_profile::solve(&identity.public_key(), 2, 0, 100_000).unwrap();
        let record = Profile {
            version: 1,
            age: age as u32,
            pow_nonce,
            pow_bits: 2,
            ..Profile::default()
        }
        .sign(&identity);
        let r = to_a.search(Request::Publish(record.clone())).await;
        assert_eq!(status(r.status), Status::Ok);
        published.push(record);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let r = to_b.search(age_query(age)).await;
        if let Some(hit) = r.hits.into_iter().next() {
            break hit;
        }
    };
    let get = Request::Get(proto::SearchGet {
        keys: vec![hit.key],
        puzzle: None,
    });
    let r = to_b.search(get).await;
    assert_eq!(r.records.len(), 1);
    assert!(published.contains(&r.records[0]));
}

#[tokio::test]
async fn digest_answers_256_bucket_hashes() {
    let addr = start_search(None).await;
    let mut client = Client::connect(&addr).await;
    for bucket in [None, Some(3)] {
        let request = proto::DigestRequest { topic: 0, bucket };
        client
            .swarm
            .behaviour_mut()
            .digest
            .send_request(&client.server, request);
        let reply = client
            .wait(|event| match event {
                BehaviourEvent::Digest(event) => response(event),
                _ => None,
            })
            .await;
        assert_eq!(status(reply.status), Status::Ok);
        // An empty index: every bucket hash is zero.
        assert_eq!(reply.hashes, vec![vec![0; 32]; 256]);
    }
}

#[tokio::test]
async fn search_queries_do_not_hold_up_the_node() {
    let addr = start_search(None).await;
    let mut client = Client::connect(&addr).await;
    for age in 0..8 {
        client.swarm.behaviour_mut().search.send_request(
            &client.server,
            proto::SearchRequest {
                request: Some(age_query(age)),
            },
        );
    }
    let info = proto::NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    client
        .swarm
        .behaviour_mut()
        .node
        .send_request(&client.server, info);
    let sent = tokio::time::Instant::now();
    let (mut answered, mut info_after) = (0, None);
    while answered < 8 || info_after.is_none() {
        let reply = client
            .wait(|event| match event {
                BehaviourEvent::Search(event) => response(event).map(Ok),
                BehaviourEvent::Node(event) => response(event).map(Err),
                _ => None,
            })
            .await;
        match reply {
            Ok(r) => {
                let busy = status(r.status) == Status::RateLimited && r.retry_after == 1;
                assert!(status(r.status) == Status::Ok || busy);
                answered += 1;
            }
            Err(_) => info_after = Some(sent.elapsed()),
        }
    }
    assert!(info_after.unwrap() < Duration::from_secs(1));
}

#[test]
fn search_worker_saturation_retries_without_stalling_info() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let addr = start_search(None).await;
        let mut client = Client::connect(&addr).await;
        let (ready, started) = tokio::sync::oneshot::channel();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = ready.send(());
            // Dropping the sender also releases this worker during a failed assertion.
            let _ = held.recv();
        });
        started.await.unwrap();
        // Two queries take the workers, four wait in the group's queue and the last is refused.
        for age in 0..7 {
            client.swarm.behaviour_mut().search.send_request(
                &client.server,
                proto::SearchRequest {
                    request: Some(age_query(age)),
                },
            );
        }
        client.swarm.behaviour_mut().node.send_request(
            &client.server,
            proto::NodeRequest {
                request: Some(node_request::Request::Info(proto::InfoRequest {})),
            },
        );
        let sent = tokio::time::Instant::now();
        let (mut busy, mut info) = (false, false);
        while !busy || !info {
            match client
                .wait(|event| match event {
                    BehaviourEvent::Search(event) => response(event).map(Ok),
                    BehaviourEvent::Node(event) => response(event).map(Err),
                    _ => None,
                })
                .await
            {
                Ok(reply) => {
                    assert_eq!(status(reply.status), Status::RateLimited);
                    assert_eq!(reply.retry_after, 1);
                    busy = true;
                }
                Err(reply) => {
                    assert_eq!(status(reply.status), Status::Ok);
                    info = true;
                }
            }
        }
        assert!(sent.elapsed() < Duration::from_secs(1));
        drop(release);
        blocker.await.unwrap();
        // Each finished query starts the next queued one.
        for _ in 0..6 {
            let reply = client
                .wait(|event| match event {
                    BehaviourEvent::Search(event) => response(event),
                    _ => None,
                })
                .await;
            assert_eq!(status(reply.status), Status::Ok);
        }
    });
}
