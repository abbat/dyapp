use super::*;

#[tokio::test]
async fn tcp_exchange_counts_exact_request_and_response_frames() {
    let (address, wire) = start_config_with_wire(|c| c.limits.requests_per_second = 100).await;
    let mut client = Client::connect(&address).await;
    wire.take();
    client.wire.take();
    let request = proto::NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    let response = client.info().await;
    let (sent, received) = (request.encoded_len() as u64, response.encoded_len() as u64);
    let slot = dyapp_p2p_net::WireBytes::protocol_slot(&dyapp_p2p_net::NODE_PROTOCOL);
    let mut expected = [[0; 2]; 10];
    expected[slot] = [sent, received];
    assert_eq!(wire.take().0, expected);
    expected[slot] = [received, sent];
    assert_eq!(client.wire.take().0, expected);
    assert_eq!(wire.take().0, [[0; 2]; 10]);
    assert_eq!(client.wire.take().0, [[0; 2]; 10]);
}

#[tokio::test]
async fn node_gossip_counts_delivered_and_successfully_published_payloads() {
    let (address, wire) = start_config_with_wire(|c| {
        c.roles = vec![Role::Store, Role::Search];
        c.search.min_profile_pow_bits = 0;
        c.search.min_heartbeat_pow_bits = 0;
        c.limits.requests_per_second = 1000;
        c.limits.bytes_per_second = 0;
        c.limits.min_free_mb = 0;
    })
    .await;
    let mut client = Client::connect_with_search(&address, Keypair::generate_ed25519(), true).await;
    let owner = Identity::generate();
    let key = dyapp_identity::key_hash(&owner.public_key());
    let topic = dyapp_p2p_net::search_topic(dyapp_bootstrap::search::topic(&key)).hash();
    client
        .wait(|event| match event {
            BehaviourEvent::Gossip(libp2p::gossipsub::Event::Subscribed {
                topic: subscribed,
                ..
            }) if subscribed == topic => Some(()),
            _ => None,
        })
        .await;
    let published = client
        .search(proto::search_request::Request::Publish(
            Profile {
                version: 1,
                ..Default::default()
            }
            .sign(&owner),
        ))
        .await;
    assert_eq!(status(published.status), Status::Ok);
    wire.take();
    let time = chrono::Utc::now().timestamp().unsigned_abs();
    let heartbeat = |time| {
        owner.sign(
            Domain::Heartbeat,
            proto::Heartbeat {
                time,
                version: 1,
                ..Default::default()
            }
            .encode_to_vec(),
        )
    };
    let incoming = proto::SearchRequest {
        request: Some(proto::search_request::Request::Heartbeat(heartbeat(time))),
    }
    .encode_to_vec();
    client
        .swarm
        .behaviour_mut()
        .gossip
        .as_mut()
        .unwrap()
        .publish(topic, incoming.clone())
        .unwrap();
    let counts = tokio::time::timeout(Duration::from_secs(10), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tokio::select! {
                _ = client.swarm.select_next_some() => {}
                _ = tick.tick() => {
                    let counts = wire.take();
                    if counts.0[dyapp_p2p_net::WireBytes::GOSSIP][0] > 0 { break counts; }
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        counts.0[dyapp_p2p_net::WireBytes::GOSSIP],
        [incoming.len() as u64, 0]
    );
    let request = proto::search_request::Request::Heartbeat(heartbeat(time + 1));
    let sent = proto::SearchRequest {
        request: Some(request.clone()),
    }
    .encoded_len() as u64;
    let response = client.search(request).await;
    assert_eq!(status(response.status), Status::Ok);
    let counts = wire.take();
    assert_eq!(counts.0[dyapp_p2p_net::WireBytes::GOSSIP], [0, sent]);
    let slot = dyapp_p2p_net::WireBytes::protocol_slot(&dyapp_p2p_net::SEARCH_PROTOCOL);
    assert_eq!(counts.0[slot], [sent, response.encoded_len() as u64]);
}
