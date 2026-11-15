//! A real libp2p profile responder with explicit request/reply gates.
use super::*;
use std::sync::Mutex;
use tokio::sync::{mpsc, oneshot};

pub(super) struct Controlled {
    pub(super) address: Multiaddr,
    pub(super) requests: mpsc::UnboundedReceiver<()>,
    pub(super) release: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
    record: Arc<Mutex<SignedRecord>>,
}

impl Drop for Controlled {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Controlled {
    pub(super) fn start(runtime: &Runtime, record: SignedRecord, paused: bool) -> Self {
        let record = Arc::new(Mutex::new(record));
        let serving = Arc::clone(&record);
        let (seen, requests) = mpsc::unbounded_channel();
        let (release, resume) = oneshot::channel();
        let (address, task) = phase(runtime, "controlled listener readiness", async {
            let mut swarm = dyapp_p2p_net::build_configured_swarm(
                Keypair::generate_ed25519(),
                dyapp_p2p_net::SwarmOptions::default(),
            )
            .unwrap();
            swarm
                .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                .unwrap();
            let address = loop {
                if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                    break address.with(Protocol::P2p(*swarm.local_peer_id()));
                }
            };
            let task = tokio::spawn(serve(swarm, serving, paused, resume, seen));
            (address, task)
        });
        Self {
            address,
            requests,
            release: Some(release),
            task,
            record,
        }
    }

    pub(super) fn replace(&self, record: SignedRecord) {
        *self.record.lock().unwrap() = record;
    }

    pub(super) fn stop(&mut self, runtime: &Runtime) {
        self.task.abort();
        let error = phase(runtime, "controlled responder shutdown", &mut self.task).unwrap_err();
        assert!(error.is_cancelled());
    }
}

async fn serve(
    mut swarm: Swarm<Behaviour>,
    record: Arc<Mutex<SignedRecord>>,
    paused: bool,
    mut resume: oneshot::Receiver<()>,
    seen: mpsc::UnboundedSender<()>,
) {
    let mut released = !paused;
    let mut pending = None;
    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                if let SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    message: Message::Request { request, channel, .. }, ..
                })) = event {
                    assert!(matches!(request.request, Some(profile_request::Request::Get(_))));
                    assert!(pending.is_none(), "only one outstanding gated request");
                    seen.send(()).unwrap();
                    if released {
                        reply(&mut swarm, channel, record.lock().unwrap().clone());
                    } else {
                        pending = Some(channel);
                    }
                }
            }
            resumed = &mut resume, if !released && pending.is_some() => {
                resumed.unwrap();
                released = true;
                reply(&mut swarm, pending.take().unwrap(), record.lock().unwrap().clone());
            }
        }
    }
}

fn reply(
    swarm: &mut Swarm<Behaviour>,
    channel: request_response::ResponseChannel<proto::ProfileResponse>,
    record: SignedRecord,
) {
    swarm
        .behaviour_mut()
        .profile
        .send_response(
            channel,
            proto::ProfileResponse {
                status: Status::Ok.into(),
                record: Some(record),
                ..Default::default()
            },
        )
        .unwrap();
}
