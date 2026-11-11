//! Readiness-driven profile/media server with reply-boundary observers.
use super::*;
use prost::Message as _;
type Server = Box<libp2p::Swarm<dyapp_p2p_net::Behaviour>>;

async fn listening() -> (Server, libp2p::Multiaddr) {
    let mut server = Box::new(
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap(),
    );
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        }
    })
    .await
    .expect("profile fixture did not become ready");
    (server, address)
}

pub(super) async fn observed(
    refuse: Vec<Status>,
    on_reply: impl FnMut(&Request, &proto::ProfileResponse) + Send + 'static,
    on_keep: impl FnMut(&proto::MediaKeep) + Send + 'static,
) -> (Node, Arc<Mutex<Option<SignedRecord>>>) {
    let (server, address) = listening().await;
    let held = Arc::new(Mutex::new(None));
    tokio::spawn(serve(server, Arc::clone(&held), refuse, on_reply, on_keep));
    let node = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .unwrap();
    (node, held)
}

async fn serve(
    mut server: Server,
    held: Arc<Mutex<Option<SignedRecord>>>,
    refuse: Vec<Status>,
    mut on_reply: impl FnMut(&Request, &proto::ProfileResponse),
    mut on_keep: impl FnMut(&proto::MediaKeep),
) {
    let mut refuse = refuse.into_iter();
    loop {
        match server.select_next_some().await {
            SwarmEvent::Behaviour(BehaviourEvent::Profile(RequestEvent::Message {
                message:
                    RequestMessage::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                let request = request.request.unwrap();
                let response = answer(&mut held.lock().unwrap(), &mut refuse, request.clone());
                on_reply(&request, &response);
                server
                    .behaviour_mut()
                    .profile
                    .send_response(channel, response)
                    .unwrap();
            }
            SwarmEvent::Behaviour(BehaviourEvent::Media(RequestEvent::Message {
                message:
                    RequestMessage::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                let Some(proto::media_request::Request::Keep(record)) = request.request else {
                    panic!("unexpected fixture media request");
                };
                let keep = proto::MediaKeep::decode(record.payload.as_slice()).unwrap();
                on_keep(&keep);
                server
                    .behaviour_mut()
                    .media
                    .send_response(
                        channel,
                        proto::MediaResponse {
                            status: Status::Stale.into(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            _ => {}
        }
    }
}
