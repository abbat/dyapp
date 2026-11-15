//! Wire fixture only: verifies client requests, not production node storage/quota policy.
use dyapp_identity::{Domain, SignedRecord};
use dyapp_p2p_net::{proto, Behaviour, BehaviourEvent, SwarmOptions};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message, ResponseChannel};
use libp2p::swarm::SwarmEvent;
use prost::Message as _;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::{timeout, timeout_at, Instant};

type Swarm = Box<libp2p::Swarm<Behaviour>>;
const LIMIT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy)]
pub(super) enum Mode {
    AttachFull,
    PutFull,
    PutTooLarge,
    MailboxFull,
    RetryMailboxFull,
    Accepted,
}

#[derive(Clone, Default)]
pub(super) struct Trace {
    pub(super) attaches: Vec<proto::MediaAttach>,
    pub(super) blobs: Vec<Vec<u8>>,
    pub(super) writes: Vec<Vec<u8>>,
    pub(super) steps: Vec<&'static str>,
}

pub(super) struct Responder {
    pub(super) address: libp2p::Multiaddr,
    trace: Arc<Mutex<Trace>>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Responder {
    pub(super) fn start(mode: Mode, owner: [u8; 32]) -> Self {
        let (ready, receive) = mpsc::channel();
        let (stop, stopped) = oneshot::channel();
        let trace = Arc::new(Mutex::new(Trace::default()));
        let copy = Arc::clone(&trace);
        let thread = std::thread::Builder::new()
            .stack_size(16 << 20)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let (swarm, address) = runtime.block_on(listening());
                ready.send(address).unwrap();
                runtime.block_on(serve(swarm, stopped, mode, owner, copy));
            })
            .unwrap();
        Self {
            address: receive
                .recv_timeout(LIMIT)
                .expect("photo responder listener readiness"),
            trace,
            stop: Some(stop),
            thread: Some(thread),
        }
    }

    pub(super) fn trace(&self) -> Trace {
        self.trace.lock().unwrap().clone()
    }

    pub(super) fn finish(mut self) -> Trace {
        let _ = self.stop.take().unwrap().send(());
        self.thread
            .take()
            .unwrap()
            .join()
            .expect("photo responder failed");
        self.trace()
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn listening() -> (Swarm, libp2p::Multiaddr) {
    let mut swarm = Box::new(
        dyapp_p2p_net::build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default())
            .unwrap(),
    );
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = timeout(LIMIT, async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                break address;
            }
        }
    })
    .await
    .expect("photo responder listen completion");
    (swarm, address)
}

async fn serve(
    mut swarm: Swarm,
    mut stop: oneshot::Receiver<()>,
    mode: Mode,
    owner: [u8; 32],
    trace: Arc<Mutex<Trace>>,
) {
    let mut deadline = Instant::now() + LIMIT;
    loop {
        let event = timeout_at(deadline, async {
            tokio::select! {
                biased;
                _ = &mut stop => None,
                event = swarm.select_next_some() => Some(event),
            }
        })
        .await
        .expect("photo responder reply completion");
        let Some(event) = event else {
            break;
        };
        if route(&mut swarm, event, mode, &owner, &trace) {
            // Only completed relevant responses renew the next transition's failure budget.
            deadline = Instant::now() + LIMIT;
        }
    }
}

fn route(
    swarm: &mut Swarm,
    event: SwarmEvent<BehaviourEvent>,
    mode: Mode,
    owner: &[u8; 32],
    trace: &Mutex<Trace>,
) -> bool {
    match event {
        SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
            message: Message::Request {
                request, channel, ..
            },
            ..
        })) => media(
            swarm,
            request,
            channel,
            mode,
            owner,
            &mut trace.lock().unwrap(),
        ),
        SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
            message: Message::Request {
                request, channel, ..
            },
            ..
        })) => mailbox(swarm, request, channel, mode, &mut trace.lock().unwrap()),
        SwarmEvent::Behaviour(
            BehaviourEvent::Media(Event::ResponseSent { .. })
            | BehaviourEvent::Mailbox(Event::ResponseSent { .. }),
        ) => return true,
        _ => {}
    }
    false
}

fn media(
    swarm: &mut Swarm,
    request: proto::MediaRequest,
    channel: ResponseChannel<proto::MediaResponse>,
    mode: Mode,
    owner: &[u8; 32],
    trace: &mut Trace,
) {
    let mut response = proto::MediaResponse {
        status: proto::Status::Ok.into(),
        ..Default::default()
    };
    match request.request.unwrap() {
        proto::media_request::Request::Attach(record) => {
            record.verify(Domain::MediaAttach).unwrap();
            assert_eq!(record.public_key.as_slice(), owner.as_slice());
            let attach = proto::MediaAttach::decode(record.payload.as_slice()).unwrap();
            assert_eq!(attach.hashes.len(), 1);
            response.missing = attach.hashes.clone();
            assert_eq!(trace.attaches.len(), 0);
            trace.attaches.push(attach);
            trace.steps.push("attach");
            if matches!(mode, Mode::AttachFull) {
                response.status = proto::Status::Full.into();
            }
        }
        proto::media_request::Request::Put(put) => {
            assert_eq!(put.owner, dyapp_identity::key_hash(owner));
            assert_eq!(
                dyapp_identity::sha256(&put.data).as_slice(),
                trace.attaches.last().unwrap().hashes[0].as_slice()
            );
            assert_eq!(trace.blobs.len(), 0);
            trace.blobs.push(put.data);
            trace.steps.push("blob");
            response.status = match mode {
                Mode::PutFull => proto::Status::Full,
                Mode::PutTooLarge => proto::Status::TooLarge,
                _ => proto::Status::Ok,
            }
            .into();
        }
        proto::media_request::Request::Get(get) => {
            assert!(matches!(mode, Mode::Accepted | Mode::MailboxFull));
            let blob = trace.blobs.last().unwrap();
            assert_eq!(get.hash, dyapp_identity::sha256(blob));
            response.data = blob.clone();
            trace.steps.push("get");
        }
        _ => panic!("unexpected photo fixture media request"),
    }
    swarm
        .behaviour_mut()
        .media
        .send_response(channel, response)
        .unwrap();
}

fn mailbox(
    swarm: &mut Swarm,
    request: proto::MailboxRequest,
    channel: ResponseChannel<proto::MailboxResponse>,
    mode: Mode,
    trace: &mut Trace,
) {
    let Some(proto::mailbox_request::Request::Put(record)) = request.request else {
        panic!("expected photo mailbox put");
    };
    verify_envelope(&record);
    if matches!(mode, Mode::MailboxFull) {
        assert_eq!(trace.writes.len(), 0);
    }
    if !matches!(mode, Mode::RetryMailboxFull) {
        assert_eq!(trace.blobs.len(), 1);
    }
    let bytes = record.encode_to_vec();
    assert!(!trace.writes.contains(&bytes));
    trace.writes.push(bytes);
    trace.steps.push("mailbox");
    let status = if matches!(mode, Mode::MailboxFull | Mode::RetryMailboxFull) {
        proto::Status::Full
    } else {
        proto::Status::Ok
    };
    swarm
        .behaviour_mut()
        .mailbox
        .send_response(
            channel,
            proto::MailboxResponse {
                status: status.into(),
                ..Default::default()
            },
        )
        .unwrap();
}

fn verify_envelope(record: &SignedRecord) {
    record.verify(Domain::Envelope).unwrap();
    let envelope = proto::Envelope::decode(record.payload.as_slice()).unwrap();
    assert_eq!(envelope.mailbox.len(), 32);
    assert_eq!(envelope.id.len(), 16);
}
