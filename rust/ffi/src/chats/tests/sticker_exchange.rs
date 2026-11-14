//! Native mailbox job runner uses core-admitted intents; verified receive metadata is injected below MLS.
use super::favorites::Fixture;
use super::offline::{establish, snapshot};
use super::saved_sticker::{pack, save};
use super::*;
use dyapp_client::chat::{pack_control::PackAnswer, sticker::Sticker};
use dyapp_client::store::messages::{MessageKey, VerifiedChatSender};
use dyapp_client::store::{sticker_packs::exchange::download::PackDownloadFailure, InboxKey};
use std::path::Path;

pub(super) struct Received {
    pub(super) native: Fixture,
    pub(super) peer: String,
    pub(super) key: MessageKey,
    pub(super) pack: dyapp_client::sticker_pack::Pack,
}

impl Received {
    pub(super) fn new() -> Self {
        let native = Fixture::new();
        let peer = establish(&native);
        let group = native
            .chats
            .session
            .lock()
            .unwrap()
            .store()
            .known_groups()
            .unwrap()
            .into_iter()
            .find(|(_, partner)| partner == &peer)
            .unwrap()
            .0;
        let key = MessageKey {
            group,
            author: peer.clone(),
            id: [42; 16],
        };
        let pack = pack();
        let received = Self {
            native,
            peer,
            key,
            pack,
        };
        received.record();
        received
    }

    fn record(&self) {
        let message = Sticker {
            id: self.key.id,
            hlc: 1,
            pack_id: *self.pack.id(),
            sha256: self.pack.stickers()[0].sha256,
            emoji: "😺".into(),
            pack_name: "Cats".into(),
            attachment: None,
        };
        let mut store = Store::open(
            &Path::new(&self.native.dir).join("client.db"),
            &self.native.key.identity,
        )
        .unwrap();
        store
            .record_chat_sticker(
                VerifiedChatSender {
                    group: &self.key.group,
                    author: &self.peer,
                    device: &[8; 32],
                },
                &message,
                true,
            )
            .unwrap();
    }

    pub(super) fn request(&self) {
        self.native
            .chats
            .session
            .lock()
            .unwrap()
            .stage_sticker_pack_request(&self.peer, &self.key, &self.native.chats.seal_key)
            .unwrap();
    }

    pub(super) fn answer(&self) -> bool {
        let (mut descriptor, _) = dyapp_client::attachment::seal(&self.pack.encode()).unwrap();
        descriptor.release_secret = [0; 32];
        let sealed = self
            .native
            .chats
            .session
            .lock()
            .unwrap()
            .store()
            .sealed_device()
            .unwrap()
            .unwrap();
        let mut store = Store::open(
            &Path::new(&self.native.dir).join("client.db"),
            &self.native.key.identity,
        )
        .unwrap();
        store
            .stage_sticker_pack_answer(
                &sealed,
                &InboxKey {
                    mailbox: [1; 32],
                    id: [77; 16],
                },
                VerifiedChatSender {
                    group: &self.key.group,
                    author: &self.peer,
                    device: &[8; 32],
                },
                &PackAnswer {
                    pack_id: *self.pack.id(),
                    attachment: descriptor,
                },
            )
            .unwrap()
    }

    pub(super) fn run(&self) -> u32 {
        let mut session = self.native.chats.session.lock().unwrap();
        self.native
            .chats
            .runtime
            .block_on(super::super::sticker_exchange::run(
                &mut session,
                &mut [],
                &self.native.key.identity,
                &self.native.chats.seal_key,
            ))
            .unwrap()
    }
}

#[test]
fn native_runner_counts_completed_save_and_keeps_hidden_controls_out_of_history() {
    let f = Received::new();
    f.request();
    assert!(f.answer());
    save(&f.native, &f.pack, &f.native.chats.seal_key);
    let before = snapshot(&f.native);
    let history = f
        .native
        .chats
        .chat_history(f.peer.clone())
        .unwrap()
        .messages;
    let items = f.native.chats.chat_items(f.peer.clone()).unwrap().len();
    assert_eq!(f.run(), 1);
    assert_eq!(f.run(), 0);
    assert_eq!(snapshot(&f.native), before);
    assert_eq!(
        f.native
            .chats
            .chat_history(f.peer.clone())
            .unwrap()
            .messages,
        history
    );
    assert_eq!(
        f.native.chats.chat_items(f.peer.clone()).unwrap().len(),
        items
    );
    let session = f.native.chats.session.lock().unwrap();
    assert!(session
        .store()
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_none());
    assert!(session
        .store()
        .saved_sticker_pack(&f.native.chats.seal_key, f.pack.id())
        .unwrap()
        .is_some());
}

#[test]
fn native_runner_retires_blocked_intent_without_saving_or_new_mail() {
    let f = Received::new();
    f.request();
    assert!(f.answer());
    f.native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .set_blocked(&f.peer, true)
        .unwrap();
    assert!(f
        .native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_some());
    let before = snapshot(&f.native);
    assert_eq!(f.run(), 0);
    assert_eq!(snapshot(&f.native), before);
    assert!(!Path::new(&f.native.dir).join("sticker-packs").exists());
    assert!(f
        .native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_none());
}

#[test]
fn native_runner_persists_fetch_failure_without_claiming_a_saved_pack() {
    let f = Received::new();
    f.request();
    assert!(f.answer());
    let before = snapshot(&f.native);
    assert_eq!(f.run(), 0);
    assert_eq!(snapshot(&f.native), before);
    let session = f.native.chats.session.lock().unwrap();
    let state = session
        .store()
        .sticker_pack_download_state(&f.peer, f.pack.id())
        .unwrap()
        .unwrap();
    assert_eq!(state.failure, Some(PackDownloadFailure::Fetch));
    assert_eq!(state.attempts, 1);
    assert!(session
        .store()
        .due_sticker_pack_downloads(state.retry_at_ms - 1)
        .unwrap()
        .is_empty());
    assert!(!Path::new(&f.native.dir).join("sticker-packs").exists());
}

#[test]
fn unsolicited_answer_never_creates_an_automatic_download_intent() {
    let f = Received::new();
    assert!(!f.answer());
    let before = snapshot(&f.native);
    assert_eq!(f.run(), 0);
    assert_eq!(snapshot(&f.native), before);
    assert!(!Path::new(&f.native.dir).join("sticker-packs").exists());
    assert!(f
        .native
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_none());
}
