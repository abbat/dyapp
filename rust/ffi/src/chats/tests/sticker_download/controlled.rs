//! A real media responder releases exactly one authenticated blob on an explicit signal.
use dyapp_p2p_net::{proto, Behaviour, BehaviourEvent};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{Event, Message};
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;

pub(super) struct Controlled {
    pub(super) seed: String,
    pub(super) requested: oneshot::Receiver<()>,
    pub(super) release: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Controlled {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Controlled {
    pub(super) fn start(runtime: &Runtime, blob: Vec<u8>) -> Self {
        let (notify, requested) = oneshot::channel();
        let (release, resume) = oneshot::channel();
        let (seed, task) = super::phase(runtime, "media listener readiness", async {
            let mut swarm = dyapp_p2p_net::build_configured_swarm(
                Keypair::generate_ed25519(),
                dyapp_p2p_net::SwarmOptions::default(),
            )
            .unwrap();
            swarm
                .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                .unwrap();
            let port = loop {
                if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                    break address
                        .iter()
                        .find_map(|part| match part {
                            Protocol::Tcp(port) => Some(port),
                            _ => None,
                        })
                        .unwrap();
                }
            };
            let seed = format!("127.0.0.1:{port}/{}", swarm.local_peer_id());
            let task = tokio::spawn(serve(swarm, blob, notify, resume));
            (seed, task)
        });
        Self {
            seed,
            requested,
            release: Some(release),
            task,
        }
    }
}

async fn serve(
    mut swarm: Swarm<Behaviour>,
    blob: Vec<u8>,
    seen: oneshot::Sender<()>,
    mut resume: oneshot::Receiver<()>,
) {
    let mut seen = Some(seen);
    let mut pending = None;
    let mut released = false;
    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                if let SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                    message: Message::Request { request, channel, .. }, ..
                })) = event {
                    let Some(proto::media_request::Request::Get(get)) = request.request else {
                        panic!("expected only the actual native media read");
                    };
                    assert_eq!(get.hash, dyapp_identity::sha256(&blob));
                    assert!(pending.is_none(), "only one outstanding gated media read");
                    pending = Some(channel);
                    seen.take().expect("one native fetch").send(()).unwrap();
                }
            }
            resumed = &mut resume, if !released && pending.is_some() => {
                resumed.unwrap();
                released = true;
                swarm.behaviour_mut().media.send_response(
                    pending.take().unwrap(),
                    proto::MediaResponse { status: proto::Status::Ok.into(), data: blob.clone(), ..Default::default() },
                ).unwrap();
            }
        }
    }
}
