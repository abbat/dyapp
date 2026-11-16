//! Actual signed mailbox traffic delivers authenticated link ciphertext to the native receiver.
use dyapp_client::link::Code;
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::{build_configured_swarm, proto, BehaviourEvent, SwarmOptions};
use libp2p::{
    futures::StreamExt as _,
    identity::Keypair,
    multiaddr::Protocol,
    request_response::{Event, Message},
    swarm::SwarmEvent,
};
use prost::Message as _;
use std::sync::mpsc;
use std::time::Duration;

pub(super) struct Mailbox {
    pub(super) network: dyapp_identity::network::Network,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Mailbox {
    pub(super) fn new(code: &Code, owner: &Identity, state: &[u8]) -> Self {
        let envelopes = code
            .seal_record(owner, state, dyapp_client::now())
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(index, ciphertext)| {
                owner.sign(
                    Domain::Envelope,
                    proto::Envelope {
                        id: u128::try_from(index).unwrap().to_be_bytes().to_vec(),
                        mailbox: code.mailbox.to_vec(),
                        ciphertext,
                        ..Default::default()
                    }
                    .encode_to_vec(),
                )
            })
            .collect::<Vec<_>>();
        let mailbox = code.mailbox;
        let (ready, listening) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(serve(envelopes, mailbox, ready, stopped));
        });
        Self {
            network: listening.recv_timeout(Duration::from_secs(5)).unwrap(),
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

impl Drop for Mailbox {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn serve(
    envelopes: Vec<SignedRecord>,
    mailbox: [u8; 32],
    ready: mpsc::Sender<dyapp_identity::network::Network>,
    mut stopped: tokio::sync::oneshot::Receiver<()>,
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
            _ = &mut stopped => break,
            event = swarm.select_next_some() => event,
        };
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                if let Some(ready) = ready.take() {
                    let port = address
                        .iter()
                        .find_map(|part| match part {
                            Protocol::Tcp(port) => Some(port),
                            _ => None,
                        })
                        .unwrap();
                    ready
                        .send(dyapp_identity::network::Network {
                            seeds: vec![format!("127.0.0.1:{port}")],
                            name: String::new(),
                        })
                        .unwrap();
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                let response = reply(request, mailbox, &envelopes);
                swarm
                    .behaviour_mut()
                    .mailbox
                    .send_response(channel, response)
                    .unwrap();
            }
            _ => {}
        }
    }
}

fn reply(
    request: proto::MailboxRequest,
    mailbox: [u8; 32],
    envelopes: &[SignedRecord],
) -> proto::MailboxResponse {
    let mut response = proto::MailboxResponse {
        status: proto::Status::Ok.into(),
        ..Default::default()
    };
    match request.request.unwrap() {
        proto::mailbox_request::Request::Challenge(_) => response.nonce = vec![7; 32],
        proto::mailbox_request::Request::Fetch(record) => {
            record.verify(Domain::MailboxFetch).unwrap();
            let key: [u8; 32] = record.public_key.try_into().unwrap();
            assert_eq!(dyapp_client::mailbox(&key), mailbox);
            let fetch = proto::Fetch::decode(record.payload.as_slice()).unwrap();
            assert_eq!(fetch.nonce, vec![7; 32]);
            assert_eq!(fetch.cursor.len(), 0);
            assert!(usize::try_from(fetch.limit).unwrap() >= envelopes.len());
            response.envelopes = envelopes.to_vec();
        }
        proto::mailbox_request::Request::Ack(record) => {
            record.verify(Domain::MailboxAck).unwrap();
            let key: [u8; 32] = record.public_key.try_into().unwrap();
            assert_eq!(dyapp_client::mailbox(&key), mailbox);
            let ack = proto::Ack::decode(record.payload.as_slice()).unwrap();
            assert_eq!(ack.nonce, vec![7; 32]);
            let ids = envelopes
                .iter()
                .map(|record| {
                    proto::Envelope::decode(record.payload.as_slice())
                        .unwrap()
                        .id
                })
                .collect::<Vec<_>>();
            assert_eq!(ack.ids, ids);
        }
        _ => panic!("unexpected receiver mailbox request"),
    }
    response
}
