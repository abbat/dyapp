use super::{prepare, prepare_put, prepare_tokened_put, Error, Node, PreparedEnvelope};
use dyapp_p2p_net::proto::{self, EnvelopeClass};
use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event as RequestEvent, Message as RequestMessage};
use libp2p::swarm::SwarmEvent;
use prost::Message as _;
use std::sync::{Arc, Mutex};

#[test]
fn a_signal_over_the_node_limit_is_refused_not_truncated() {
    let signal = prepare(&[7; 32], vec![0; 900], EnvelopeClass::Signal, None).unwrap();
    assert_eq!(signal.envelope().class(), EnvelopeClass::Signal);
    assert!(matches!(
        prepare(&[7; 32], vec![0; 1024], EnvelopeClass::Signal, None),
        Err(Error::Input(_))
    ));
    prepare(&[7; 32], vec![0; 1024], EnvelopeClass::Message, None).unwrap();
}

#[test]
fn prepared_mailbox_write_reopens_with_exact_signed_bytes() {
    let mailbox = [7; 32];
    let prepared = prepare_put(&mailbox, vec![1, 2, 3]).unwrap();
    let bytes = prepared.as_bytes().to_vec();
    let reopened = PreparedEnvelope::from_bytes(bytes.clone()).unwrap();
    assert_eq!(reopened.as_bytes(), bytes);
    assert_eq!(reopened.mailbox(), &mailbox);
    assert_eq!(reopened.id(), prepared.id());
    let envelope = proto::Envelope::decode(reopened.record.payload.as_slice()).unwrap();
    assert_eq!(envelope.ciphertext, [1, 2, 3]);
    assert_eq!(envelope.class, EnvelopeClass::Message as i32);
    assert_ne!(
        prepare_put(&mailbox, vec![1, 2, 3]).unwrap().id(),
        prepared.id()
    );
    assert_eq!(
        format!("{prepared:?}"),
        format!("PreparedEnvelope {{ bytes_len: {}, .. }}", bytes.len())
    );
}

#[test]
fn a_tokened_write_carries_the_token_and_is_signed_by_its_key() {
    let token_key = dyapp_identity::Identity::generate();
    let token = dyapp_identity::Identity::generate()
        .sign(dyapp_identity::Domain::WriteToken, vec![1])
        .encode_to_vec();
    let prepared = prepare_tokened_put(&[7; 32], vec![1], &token_key, &token).unwrap();
    assert_eq!(prepared.record.public_key, token_key.public_key());
    let envelope = proto::Envelope::decode(prepared.record.payload.as_slice()).unwrap();
    assert_eq!(envelope.write_token.unwrap().encode_to_vec(), token);
    assert!(proto::Envelope::decode(
        prepare_put(&[7; 32], vec![1])
            .unwrap()
            .record
            .payload
            .as_slice()
    )
    .unwrap()
    .write_token
    .is_none());
    assert!(matches!(
        prepare_tokened_put(&[7; 32], vec![1], &token_key, &[0xff]),
        Err(Error::Input(_))
    ));
}

#[test]
fn prepared_mailbox_write_rejects_wrong_address_and_tampering() {
    assert!(matches!(
        prepare_put(&[0; 31], vec![1]),
        Err(Error::Input(_))
    ));
    let prepared = prepare_put(&[0; 32], vec![1]).unwrap();
    let mut bytes = prepared.as_bytes().to_vec();
    *bytes.last_mut().unwrap() ^= 1;
    assert!(matches!(
        PreparedEnvelope::from_bytes(bytes),
        Err(Error::PreparedEnvelope)
    ));
}

#[tokio::test]
async fn node_put_and_prepared_retry_reach_the_mailbox() {
    let mut server =
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let received = Arc::new(Mutex::new(Vec::new()));
    let copy = Arc::clone(&received);
    let task = tokio::spawn(async move {
        loop {
            if let SwarmEvent::Behaviour(BehaviourEvent::Mailbox(RequestEvent::Message {
                message:
                    RequestMessage::Request {
                        request, channel, ..
                    },
                ..
            })) = server.select_next_some().await
            {
                let Some(proto::mailbox_request::Request::Put(record)) = request.request else {
                    panic!("expected mailbox put");
                };
                copy.lock().unwrap().push(record);
                server
                    .behaviour_mut()
                    .mailbox
                    .send_response(
                        channel,
                        proto::MailboxResponse {
                            status: proto::Status::Ok.into(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
        }
    });
    let mut node = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .unwrap();
    let mailbox = [7; 32];
    let prepared = prepare_put(&mailbox, vec![1]).unwrap();
    node.put_prepared(&prepared).await.unwrap();
    node.put_prepared(&prepared).await.unwrap();
    let fresh_id = node.put(&mailbox, vec![2]).await.unwrap();
    let records = received.lock().unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].encode_to_vec(), prepared.as_bytes());
    assert_eq!(records[1].encode_to_vec(), prepared.as_bytes());
    let fresh = proto::Envelope::decode(records[2].payload.as_slice()).unwrap();
    assert_eq!(fresh.id, fresh_id);
    assert_eq!(fresh.mailbox, mailbox);
    assert_eq!(fresh.ciphertext, [2]);
    // Other tests share the process counter, so only its presence and growth are certain here.
    assert!(
        crate::traffic().is_some_and(|[i, out]| i > 0 && out >= prepared.as_bytes().len() as u64)
    );
    task.abort();
}

#[tokio::test]
async fn providers_are_dialed_for_their_addresses() {
    use libp2p::kad::{self, store::RecordStore};
    dyapp_p2p_net::set_id_pow_bits(0);
    let mut server =
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    // A confirmed address puts Auto-mode Kademlia in server mode, so it answers lookups.
    server.add_external_address(address.clone());
    let key = kad::RecordKey::new(&dyapp_p2p_net::SEARCH_KEY);
    server
        .behaviour_mut()
        .kad
        .start_providing(key.clone())
        .unwrap();
    // A second provider that refuses connections.
    let gone = kad::ProviderRecord::new(
        key,
        libp2p::PeerId::random(),
        vec!["/ip4/127.0.0.1/tcp/1".parse().unwrap()],
    );
    server
        .behaviour_mut()
        .kad
        .store_mut()
        .add_provider(gone)
        .unwrap();
    let task = tokio::spawn(async move {
        loop {
            server.select_next_some().await;
        }
    });
    let mut node = Node::connect(address.clone(), Arc::new(Protocols::public()))
        .await
        .unwrap();
    let address = address.with(libp2p::multiaddr::Protocol::P2p(node.peer()));

    // The refused dial ends the lookup well before the timeout.
    let started = std::time::Instant::now();
    assert_eq!(
        node.providers(dyapp_p2p_net::SEARCH_KEY, 3).await,
        std::slice::from_ref(&address)
    );
    assert_eq!(
        node.providers(dyapp_p2p_net::SEARCH_KEY, 1).await,
        [address]
    );
    assert!(node
        .providers(dyapp_p2p_net::SEARCH_KEY, 0)
        .await
        .is_empty());
    assert!(node.providers(dyapp_p2p_net::TURN_KEY, 3).await.is_empty());
    assert!(started.elapsed() < super::TIMEOUT);
    // A node without the node-ID proof of work is not dialed.
    dyapp_p2p_net::set_id_pow_bits(200);
    assert!(node
        .providers(dyapp_p2p_net::SEARCH_KEY, 3)
        .await
        .is_empty());
    task.abort();
}

#[tokio::test]
async fn search_calls_solve_the_puzzle_and_surface_busy_nodes() {
    use proto::{search_request::Request, SearchResponse, Status};
    let reply = |status: Status| SearchResponse {
        status: status.into(),
        ..Default::default()
    };
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
    let mut replies = vec![
        SearchResponse {
            puzzle_challenge: vec![3; 16],
            puzzle_bits: 8,
            ..reply(Status::Puzzle)
        },
        SearchResponse {
            hits: vec![proto::SearchHit {
                key: vec![1; 32],
                version: 2,
            }],
            ..reply(Status::Ok)
        },
        SearchResponse {
            partial: true,
            ..reply(Status::Ok)
        },
        SearchResponse {
            retry_after: 30,
            ..reply(Status::RateLimited)
        },
        reply(Status::Denied),
        SearchResponse {
            puzzle_challenge: vec![3; 16],
            puzzle_bits: super::MAX_PUZZLE_BITS + 1,
            ..reply(Status::Puzzle)
        },
        SearchResponse {
            puzzle_challenge: vec![3; 15],
            puzzle_bits: super::MAX_PUZZLE_BITS,
            ..reply(Status::Puzzle)
        },
    ]
    .into_iter();
    let received = Arc::new(Mutex::new(Vec::new()));
    let copy = Arc::clone(&received);
    let task = tokio::spawn(async move {
        loop {
            if let SwarmEvent::Behaviour(BehaviourEvent::Search(RequestEvent::Message {
                peer,
                message:
                    RequestMessage::Request {
                        request, channel, ..
                    },
                ..
            })) = server.select_next_some().await
            {
                copy.lock().unwrap().push((peer, request.request.unwrap()));
                let response = replies.next().unwrap();
                server
                    .behaviour_mut()
                    .search
                    .send_response(channel, response)
                    .unwrap();
            }
        }
    });
    let mut node = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .unwrap();
    let hits = node.search_query(Vec::new(), 7).await.unwrap().hits;
    assert_eq!(hits.len(), 1);
    assert!(node.search_get(vec![vec![1; 32]]).await.unwrap().partial);
    let record = dyapp_identity::Identity::generate().sign(dyapp_identity::Domain::Profile, vec![]);
    assert!(matches!(
        node.search_publish(record.clone()).await,
        Err(Error::RateLimited(after)) if after.as_secs() == 30
    ));
    assert!(matches!(
        node.search_heartbeat(record).await,
        Err(Error::Status(Status::Denied))
    ));
    assert!(matches!(
        node.search_get(Vec::new()).await,
        Err(Error::Reply("puzzle too hard"))
    ));
    assert!(matches!(
        node.search_query(Vec::new(), 0).await,
        Err(Error::Reply("bad puzzle challenge"))
    ));
    let received = received.lock().unwrap();
    let kinds: Vec<_> = received
        .iter()
        .map(|(_, request)| match request {
            Request::Publish(_) => "publish",
            Request::Heartbeat(_) => "heartbeat",
            Request::Query(_) => "query",
            Request::Get(_) => "get",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "query",
            "query",
            "get",
            "publish",
            "heartbeat",
            "get",
            "query"
        ]
    );
    let (peer, (Request::Query(first), Request::Query(solved), Request::Get(get))) = (
        received[0].0,
        (&received[0].1, &received[1].1, &received[2].1),
    ) else {
        panic!("unexpected requests {received:?}");
    };
    assert_eq!((first.limit, first.puzzle.as_ref()), (7, None));
    let puzzle = solved.puzzle.as_ref().unwrap();
    assert_eq!(puzzle.challenge, [3; 16]);
    assert!(dyapp_p2p_net::puzzle::solved(
        &puzzle.challenge,
        &peer.to_base58(),
        puzzle.nonce,
        8
    ));
    assert_eq!(get.keys, [vec![1; 32]]);
    task.abort();
}
