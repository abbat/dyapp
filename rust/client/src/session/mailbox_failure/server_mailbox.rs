//! A retained envelope exposes accidental application or acknowledgement by a probe.
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto;
use prost::Message as _;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

pub(super) struct Mailbox {
    pub(super) status: AtomicI32,
    pub(super) fetches: AtomicUsize,
    pub(super) acknowledgements: AtomicUsize,
    pub(super) pending: AtomicBool,
    sender: Identity,
}

impl Default for Mailbox {
    fn default() -> Self {
        Self {
            status: AtomicI32::new(proto::Status::Ok.into()),
            fetches: AtomicUsize::new(0),
            acknowledgements: AtomicUsize::new(0),
            pending: AtomicBool::new(true),
            sender: Identity::generate(),
        }
    }
}

impl Mailbox {
    pub(super) fn reply(&self, request: proto::MailboxRequest) -> proto::MailboxResponse {
        match request.request.unwrap() {
            proto::mailbox_request::Request::Challenge(_) => proto::MailboxResponse {
                status: proto::Status::Ok.into(),
                nonce: vec![7; 32],
                ..Default::default()
            },
            proto::mailbox_request::Request::Fetch(record) => self.fetch(&record),
            proto::mailbox_request::Request::Ack(record) => {
                record.verify(Domain::MailboxAck).unwrap();
                self.acknowledgements.fetch_add(1, Ordering::SeqCst);
                self.pending.store(false, Ordering::SeqCst);
                proto::MailboxResponse {
                    status: proto::Status::Ok.into(),
                    ..Default::default()
                }
            }
            _ => panic!("unexpected revocation-probe request"),
        }
    }

    fn fetch(&self, record: &SignedRecord) -> proto::MailboxResponse {
        record.verify(Domain::MailboxFetch).unwrap();
        let fetch = proto::Fetch::decode(record.payload.as_slice()).unwrap();
        assert_eq!(fetch.nonce, vec![7; 32]);
        assert_eq!(fetch.limit, 1);
        assert_eq!(fetch.cursor.len(), 0);
        assert!(!fetch.watch);
        self.fetches.fetch_add(1, Ordering::SeqCst);
        let status = self.status.load(Ordering::SeqCst);
        let envelopes =
            if status == i32::from(proto::Status::Ok) && self.pending.load(Ordering::SeqCst) {
                vec![self.envelope(&record.public_key)]
            } else {
                vec![]
            };
        proto::MailboxResponse {
            status,
            envelopes,
            ..Default::default()
        }
    }

    fn envelope(&self, key: &[u8]) -> SignedRecord {
        self.sender.sign(
            Domain::Envelope,
            proto::Envelope {
                id: vec![9; 16],
                mailbox: crate::mailbox(key.try_into().unwrap()).to_vec(),
                ciphertext: b"retained opaque ciphertext".to_vec(),
                ..Default::default()
            }
            .encode_to_vec(),
        )
    }
}
