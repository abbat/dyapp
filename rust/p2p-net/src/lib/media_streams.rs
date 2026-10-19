//! Media stream limits per connection and across connections.

use super::*;
use libp2p::futures::StreamExt;

#[tokio::test]
async fn third_media_stream_on_a_connection_is_reset() {
    media_stream_limit(1, 3, 2).await;
}

#[tokio::test]
async fn seventeenth_media_stream_across_connections_is_reset() {
    media_stream_limit(9, 17, 16).await;
}

async fn media_stream_limit(peers: usize, requests: usize, accepted: usize) {
    let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let server_id = *server.local_peer_id();
    let mut clients: Vec<_> = (0..peers)
        .map(|_| {
            let mut client = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
            // Senders allow all requests; the receiver enforces its own limits.
            client.behaviour_mut().media = request_response::Behaviour::new(
                [(MEDIA_PROTOCOL, ProtocolSupport::Outbound)],
                request_response::Config::default().with_max_concurrent_streams(100),
            );
            client.add_peer_address(server_id, addr.clone());
            client
        })
        .collect();
    for i in 0..requests {
        clients[i % peers]
            .behaviour_mut()
            .media
            .send_request(&server_id, proto::MediaRequest::default());
    }
    let mut held = Vec::new();
    let mut rejected = false;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                event = server.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Media(request_response::Event::Message {
                        message: request_response::Message::Request { channel, .. }, ..
                    })) = event {
                        held.push(channel);
                        assert!(held.len() <= accepted);
                    }
                }
                event = async { libp2p::futures::stream::select_all(clients.iter_mut()).select_next_some().await } => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Media(request_response::Event::OutboundFailure {
                        error, ..
                    })) = event {
                        assert!(matches!(error, request_response::OutboundFailure::Io(_)), "{error:?}");
                        rejected = true;
                    }
                }
            }
            if rejected && held.len() == accepted {
                break;
            }
        }
    }).await.expect("excess media stream was queued instead of reset");
    assert_eq!(held.len(), accepted);
    for channel in held {
        server
            .behaviour_mut()
            .media
            .send_response(
                channel,
                proto::MediaResponse {
                    status: proto::Status::Ok.into(),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let mut replies = 0;
    tokio::time::timeout(Duration::from_secs(10), async {
        while replies < accepted {
            tokio::select! {
                _ = server.select_next_some() => {}
                event = async { libp2p::futures::stream::select_all(clients.iter_mut()).select_next_some().await } => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Media(request_response::Event::Message {
                        message: request_response::Message::Response { response, .. }, ..
                    })) = event {
                        assert_eq!(response.status, proto::Status::Ok as i32);
                        replies += 1;
                    }
                }
            }
        }
    }).await.expect("accepted media streams stopped working");
}
