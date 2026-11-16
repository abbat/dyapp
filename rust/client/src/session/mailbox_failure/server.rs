//! Controlled profile readback over the actual client protocol.
use super::server_mailbox::Mailbox;
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::{build_configured_swarm, proto, Behaviour, BehaviourEvent, SwarmOptions};
use libp2p::{
    futures::StreamExt as _,
    identity::Keypair,
    request_response::{Event, Message},
    swarm::SwarmEvent,
    Multiaddr, Swarm,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::Duration;

pub(super) struct Server {
    pub(super) address: Multiaddr,
    pub(super) record: Arc<Mutex<Option<SignedRecord>>>,
    pub(super) reads: Arc<AtomicUsize>,
    pub(super) mailbox: Arc<Mailbox>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    pub(super) fn new(record: SignedRecord) -> Self {
        let record = Arc::new(Mutex::new(Some(record)));
        let reads = Arc::new(AtomicUsize::new(0));
        let mailbox = Arc::new(Mailbox::default());
        let (endpoint_notice, listening) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let responses = Arc::clone(&record);
        let requests = Arc::clone(&reads);
        let delivery = Arc::clone(&mailbox);
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(serve(
                    responses,
                    requests,
                    delivery,
                    endpoint_notice,
                    stopped,
                ));
        });
        Self {
            address: listening.recv_timeout(Duration::from_secs(5)).unwrap(),
            record,
            reads,
            mailbox,
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

fn response(
    request: &proto::ProfileRequest,
    record: Option<SignedRecord>,
) -> proto::ProfileResponse {
    assert!(matches!(
        request.request,
        Some(proto::profile_request::Request::Get(_))
    ));
    let status = if record.is_some() {
        proto::Status::Ok
    } else {
        proto::Status::NotFound
    };
    proto::ProfileResponse {
        status: status.into(),
        record,
        ..Default::default()
    }
}

fn route(
    swarm: &mut Swarm<Behaviour>,
    event: BehaviourEvent,
    record: &Mutex<Option<SignedRecord>>,
    reads: &AtomicUsize,
    mailbox: &Mailbox,
) {
    match event {
        BehaviourEvent::Profile(Event::Message {
            message: Message::Request {
                request, channel, ..
            },
            ..
        }) => {
            reads.fetch_add(1, Ordering::SeqCst);
            let response = response(&request, record.lock().unwrap().clone());
            swarm
                .behaviour_mut()
                .profile
                .send_response(channel, response)
                .unwrap();
        }
        BehaviourEvent::Mailbox(Event::Message {
            message: Message::Request {
                request, channel, ..
            },
            ..
        }) => {
            swarm
                .behaviour_mut()
                .mailbox
                .send_response(channel, mailbox.reply(request))
                .unwrap();
        }
        _ => {}
    }
}

async fn serve(
    record: Arc<Mutex<Option<SignedRecord>>>,
    reads: Arc<AtomicUsize>,
    mailbox: Arc<Mailbox>,
    endpoint_notice: mpsc::Sender<Multiaddr>,
    mut stopped: tokio::sync::oneshot::Receiver<()>,
) {
    let mut endpoint_notice = Some(endpoint_notice);
    let mut swarm = Box::new(
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap(),
    );
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        let event = tokio::select! {
            _ = &mut stopped => break,
            event = swarm.select_next_some() => event,
        };
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                if let Some(endpoint_notice) = endpoint_notice.take() {
                    endpoint_notice.send(address).unwrap();
                }
            }
            SwarmEvent::Behaviour(event) => route(&mut swarm, event, &record, &reads, &mailbox),
            _ => {}
        }
    }
}
