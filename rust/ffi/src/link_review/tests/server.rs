//! Every reply can complete one queued mutation before the caller resumes.
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::{build_configured_swarm, proto, BehaviourEvent, SwarmOptions};
use libp2p::{
    futures::StreamExt as _,
    identity::Keypair,
    multiaddr::Protocol,
    request_response::{Event, Message},
    swarm::SwarmEvent,
};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub(super) type Callback = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;
pub(super) struct Server {
    pub(super) network: dyapp_identity::network::Network,
    pub(super) record: Arc<Mutex<SignedRecord>>,
    pub(super) on_read: Callback,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub(super) fn new(record: SignedRecord) -> Self {
        let record = Arc::new(Mutex::new(record));
        let on_read: Callback = Arc::default();
        let (ready, listening) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let held = Arc::clone(&record);
        let callback = Arc::clone(&on_read);
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(serve(held, callback, ready, stopped));
        });
        Self {
            network: listening.recv_timeout(Duration::from_secs(5)).unwrap(),
            record,
            on_read,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn listening_network(address: &libp2p::Multiaddr) -> dyapp_identity::network::Network {
    let port = address
        .iter()
        .find_map(|protocol| match protocol {
            Protocol::Tcp(port) => Some(port),
            _ => None,
        })
        .unwrap();
    dyapp_identity::network::Network {
        seeds: vec![format!("127.0.0.1:{port}")],
        name: String::new(),
    }
}

async fn serve(
    record: Arc<Mutex<SignedRecord>>,
    callback: Callback,
    ready: mpsc::Sender<dyapp_identity::network::Network>,
    mut stop: tokio::sync::oneshot::Receiver<()>,
) {
    let mut ready = Some(ready);
    let mut swarm = Box::new(
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap(),
    );
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        let event = tokio::select! {
            _ = &mut stop => break,
            event = swarm.select_next_some() => event,
        };
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                if let Some(ready) = ready.take() {
                    ready.send(listening_network(&address)).unwrap();
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                assert!(matches!(
                    request.request,
                    Some(proto::profile_request::Request::Get(_))
                ));
                let response = proto::ProfileResponse {
                    status: proto::Status::Ok.into(),
                    record: Some(record.lock().unwrap().clone()),
                    ..Default::default()
                };
                let action = callback.lock().unwrap().take();
                if let Some(action) = action {
                    action();
                }
                swarm
                    .behaviour_mut()
                    .profile
                    .send_response(channel, response)
                    .unwrap();
            }
            _ => {}
        }
    }
}
