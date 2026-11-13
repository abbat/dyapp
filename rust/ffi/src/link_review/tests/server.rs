//! Replies expose actual request boundaries for deterministic recovery tests.
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::{build_configured_swarm, proto, Behaviour, BehaviourEvent, SwarmOptions};
use libp2p::{
    futures::StreamExt as _,
    identity::Keypair,
    multiaddr::Protocol,
    request_response::{Event, Message},
    swarm::SwarmEvent,
    Swarm,
};
use prost::Message as _;
use std::sync::{
    atomic::{AtomicI32, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::Duration;

pub(super) type Callback = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

pub(super) struct Packages {
    pub(super) records: Mutex<Vec<SignedRecord>>,
    pub(super) ordinary_status: AtomicI32,
    pub(super) fallback_status: AtomicI32,
    pub(super) on_write: Callback,
}
impl Default for Packages {
    fn default() -> Self {
        Self {
            records: Mutex::default(),
            ordinary_status: AtomicI32::new(proto::Status::Ok.into()),
            fallback_status: AtomicI32::new(proto::Status::Ok.into()),
            on_write: Arc::default(),
        }
    }
}

#[derive(Clone)]
struct Replies {
    record: Arc<Mutex<SignedRecord>>,
    on_read: Callback,
    packages: Arc<Packages>,
    profile_writes: Arc<AtomicUsize>,
}
pub(super) struct Server {
    pub(super) network: dyapp_identity::network::Network,
    pub(super) record: Arc<Mutex<SignedRecord>>,
    pub(super) on_read: Callback,
    pub(super) packages: Arc<Packages>,
    pub(super) profile_writes: Arc<AtomicUsize>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub(super) fn new(record: SignedRecord) -> Self {
        let replies = Replies {
            record: Arc::new(Mutex::new(record)),
            on_read: Arc::default(),
            packages: Arc::default(),
            profile_writes: Arc::default(),
        };
        let (ready, listening) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let held = replies.clone();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(serve(held, ready, stopped));
        });
        Self {
            network: listening.recv_timeout(Duration::from_secs(5)).unwrap(),
            record: replies.record,
            on_read: replies.on_read,
            packages: replies.packages,
            profile_writes: replies.profile_writes,
            stop: Some(stop),
            thread: Some(thread),
        }
    }

    pub(super) fn shutdown(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
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
    replies: Replies,
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
            SwarmEvent::Behaviour(event) => reply(&mut swarm, event, &replies),
            _ => {}
        }
    }
}

fn reply(swarm: &mut Swarm<Behaviour>, event: BehaviourEvent, replies: &Replies) {
    match event {
        BehaviourEvent::Profile(Event::Message {
            message: Message::Request {
                request, channel, ..
            },
            ..
        }) => {
            let response = profile_reply(replies, &request);
            swarm
                .behaviour_mut()
                .profile
                .send_response(channel, response)
                .unwrap();
        }
        BehaviourEvent::KeyPackage(Event::Message {
            message: Message::Request {
                request, channel, ..
            },
            ..
        }) => {
            let response = package_reply(&replies.packages, request);
            swarm
                .behaviour_mut()
                .key_package
                .send_response(channel, response)
                .unwrap();
        }
        _ => {}
    }
}

fn profile_reply(replies: &Replies, request: &proto::ProfileRequest) -> proto::ProfileResponse {
    if matches!(
        request.request,
        Some(
            proto::profile_request::Request::Publish(_)
                | proto::profile_request::Request::ConditionalPublish(_)
        )
    ) {
        replies.profile_writes.fetch_add(1, Ordering::SeqCst);
        return proto::ProfileResponse {
            status: proto::Status::Refused.into(),
            ..Default::default()
        };
    }
    assert!(matches!(
        request.request,
        Some(proto::profile_request::Request::Get(_))
    ));
    let response = proto::ProfileResponse {
        status: proto::Status::Ok.into(),
        record: Some(replies.record.lock().unwrap().clone()),
        ..Default::default()
    };
    callback(&replies.on_read);
    response
}

fn package_reply(
    packages: &Packages,
    request: proto::KeyPackageRequest,
) -> proto::KeyPackageResponse {
    let Some(proto::key_package_request::Request::Publish(record)) = request.request else {
        panic!("recovery should publish key packages");
    };
    let payload = proto::KeyPackageRecord::decode(record.payload.as_slice()).unwrap();
    let status = if payload.last_resort {
        packages.fallback_status.load(Ordering::SeqCst)
    } else {
        packages.ordinary_status.load(Ordering::SeqCst)
    };
    if status == i32::from(proto::Status::Ok) {
        packages.records.lock().unwrap().push(record);
    }
    callback(&packages.on_write);
    proto::KeyPackageResponse {
        status,
        ..Default::default()
    }
}

fn callback(callback: &Callback) {
    let action = callback.lock().unwrap().take();
    if let Some(action) = action {
        action();
    }
}
