//! Readiness-driven profile/media server with reply-boundary observers.
use super::*;
use prost::Message as _;
type Server = Box<libp2p::Swarm<dyapp_p2p_net::Behaviour>>;

fn media_reply(
    record: SignedRecord,
    state: &mut proto::MediaKeepState,
    on_keep: &mut impl FnMut(&proto::MediaKeep),
) -> proto::MediaResponse {
    assert!(record
        .verify(dyapp_identity::Domain::MediaConditionalKeep)
        .is_ok());
    let intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    let mut response = proto::MediaResponse {
        status: Status::Ok.into(),
        ..Default::default()
    };
    if let Some(expected) = intent.expected {
        let hash = state.record.as_ref().map_or_else(Vec::new, |held| {
            dyapp_identity::sha256(&held.encode_to_vec()).to_vec()
        });
        if expected.version != state.version
            || expected.owner_present != state.owner_present
            || expected.record_hash != hash
        {
            response.status = Status::Stale.into();
        } else {
            on_keep(&proto::MediaKeep::decode(record.payload.as_slice()).unwrap());
            *state = proto::MediaKeepState {
                owner_present: true,
                version: intent.version,
                record: Some(record),
            };
            response.keep_applied = true;
        }
    }
    response.keep_state = Some(Box::new(state.clone()));
    response
}

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

fn answer_mailbox(
    server: &mut Server,
    request: &proto::MailboxRequest,
    channel: libp2p::request_response::ResponseChannel<proto::MailboxResponse>,
) {
    assert!(matches!(
        request.request,
        Some(proto::mailbox_request::Request::Revoke(_))
    ));
    server
        .behaviour_mut()
        .mailbox
        .send_response(
            channel,
            proto::MailboxResponse {
                status: Status::Ok.into(),
                ..Default::default()
            },
        )
        .unwrap();
}

fn conditional_reply(
    request: proto::MediaRequest,
    state: &mut proto::MediaKeepState,
    on_keep: &mut impl FnMut(&proto::MediaKeep),
) -> proto::MediaResponse {
    let Some(proto::media_request::Request::ConditionalKeep(record)) = request.request else {
        panic!("unexpected fixture media request");
    };
    media_reply(record, state, on_keep)
}

async fn serve(
    mut server: Server,
    held: Arc<Mutex<Option<SignedRecord>>>,
    refuse: Vec<Status>,
    mut on_reply: impl FnMut(&Request, &proto::ProfileResponse),
    mut on_keep: impl FnMut(&proto::MediaKeep),
) {
    let mut refuse = refuse.into_iter();
    let mut media = proto::MediaKeepState::default();
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
            SwarmEvent::Behaviour(BehaviourEvent::Mailbox(RequestEvent::Message {
                message:
                    RequestMessage::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                answer_mailbox(&mut server, &request, channel);
            }
            SwarmEvent::Behaviour(BehaviourEvent::Media(RequestEvent::Message {
                message:
                    RequestMessage::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                let response = conditional_reply(request, &mut media, &mut on_keep);
                server
                    .behaviour_mut()
                    .media
                    .send_response(channel, response)
                    .unwrap();
            }
            _ => {}
        }
    }
}
