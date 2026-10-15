use super::*;

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
        for age in 0..3 {
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
        for _ in 0..2 {
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
