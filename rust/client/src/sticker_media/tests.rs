use super::*;
use crate::chat::{sticker::Sticker, Body};
use crate::store::messages::VerifiedChatSender;
use crate::store::InboxKey;
use std::path::PathBuf;

const SEAL: [u8; 32] = [5; 32];

fn register(store: &mut Store, peer: &str, open: bool, sticker: &Sticker) {
    crate::store::tests::catalog_group(store, &[9], peer);
    let sender = VerifiedChatSender {
        group: &[9],
        author: peer,
        device: &[8; 32],
    };
    store
        .stage_chat_inbound(
            b"mls",
            &InboxKey {
                mailbox: [1; 32],
                id: [9; 16],
            },
            sender,
            &Body::text_with_id([9; 16], b"hello".to_vec()),
            1_000,
        )
        .unwrap();
    if open {
        crate::forwarding::tests::open_chat(store, peer);
    }
    store.record_chat_sticker(sender, sticker, true).unwrap();
}

struct Fixture {
    store: Store,
    peer: String,
    dir: PathBuf,
    key: MessageKey,
    bytes: Vec<u8>,
    pack: crate::sticker_pack::Pack,
}

impl Fixture {
    fn new(open: bool, attached: bool) -> Self {
        let owner = dyapp_identity::Identity::generate();
        let peer = dyapp_identity::Identity::generate().peer_id();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/sticker-media-core-tests")
            .join(owner.peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open(&dir.join("client.db"), &owner).unwrap();
        let pack = crate::store::sticker_packs::tests::pack("Cats");
        let bytes = pack.sticker_bytes(0).unwrap().to_vec();
        let (mut descriptor, _) = attachment::seal(&bytes).unwrap();
        descriptor.mime = "image/webp".into();
        descriptor.width = 1;
        descriptor.height = 1;
        descriptor.release_secret = [0; 32];
        let sticker = Sticker {
            id: [1; 16],
            hlc: 1,
            pack_id: *pack.id(),
            sha256: pack.stickers()[0].sha256,
            emoji: "😺".into(),
            pack_name: "Cats".into(),
            attachment: attached.then_some(descriptor),
        };
        register(&mut store, &peer, open, &sticker);
        let key = MessageKey {
            group: vec![9],
            author: peer.clone(),
            id: [1; 16],
        };
        Self {
            store,
            peer,
            dir,
            key,
            bytes,
            pack,
        }
    }

    fn select(&self) -> Snapshot {
        select(&self.store, &self.peer, &self.key).unwrap()
    }
}

#[test]
fn selection_retains_full_key_and_refuses_own_blocked_or_unknown_context() {
    let f = Fixture::new(true, true);
    assert!(f.select().message().key == f.key);
    for key in [
        MessageKey {
            group: vec![10],
            ..f.key.clone()
        },
        MessageKey {
            author: "other".into(),
            ..f.key.clone()
        },
        MessageKey {
            id: [9; 16],
            ..f.key.clone()
        },
    ] {
        assert!(matches!(
            select(&f.store, &f.peer, &key),
            Err(Error::UnknownMessage)
        ));
    }
    assert!(matches!(
        select(&f.store, f.store.owner_peer_id(), &f.key),
        Err(Error::OwnPeer)
    ));
    assert!(matches!(
        select(&f.store, "missing", &f.key),
        Err(Error::UnknownPeer)
    ));
    let mut changed = f.select();
    changed.item.sticker.emoji = "🐶".into();
    assert!(matches!(
        changed.finish(&f.store, &f.dir, &SEAL, &f.bytes),
        Err(Error::UnknownMessage)
    ));
    f.store.set_blocked(&f.peer, true).unwrap();
    assert!(matches!(
        select(&f.store, &f.peer, &f.key),
        Err(Error::Blocked)
    ));
}

#[test]
fn current_completion_refuses_block_clear_wrong_identity_and_corrupt_pixels() {
    let mut f = Fixture::new(true, true);
    let snapshot = f.select();
    assert!(matches!(
        snapshot.finish(&f.store, &f.dir, &SEAL, b"bad bytes"),
        Err(Error::Corrupted)
    ));
    assert!(!f.dir.join("originals").exists());
    f.store.set_blocked(&f.peer, true).unwrap();
    assert!(matches!(
        snapshot.finish(&f.store, &f.dir, &SEAL, &f.bytes),
        Err(Error::Blocked)
    ));
    f.store.set_blocked(&f.peer, false).unwrap();
    let foreign = Fixture::new(true, true);
    assert!(matches!(
        snapshot.finish(&foreign.store, &foreign.dir, &SEAL, &f.bytes),
        Err(Error::IdentityChanged)
    ));
    f.store.clear_chat_pair(&f.peer).unwrap();
    assert!(matches!(
        snapshot.finish(&f.store, &f.dir, &SEAL, &f.bytes),
        Err(Error::UnknownMessage)
    ));
    assert!(!f.dir.join("originals").exists());
}

#[test]
fn saved_fallback_and_authenticated_cache_return_only_exact_core_pixels() {
    let mut saved = Fixture::new(true, false);
    saved.store.save_sticker_pack(&SEAL, &saved.pack).unwrap();
    let frame = saved
        .select()
        .local(&saved.store, &saved.dir, &SEAL)
        .unwrap()
        .unwrap();
    assert_eq!(
        (frame.width, frame.height, frame.pixels),
        (1, 1, vec![1, 2, 3, 255])
    );
    let cached = Fixture::new(true, true);
    let snapshot = cached.select();
    assert!(snapshot
        .local(&cached.store, &cached.dir, &SEAL)
        .unwrap()
        .is_none());
    snapshot
        .finish(&cached.store, &cached.dir, &SEAL, &cached.bytes)
        .unwrap();
    let frame = snapshot
        .local(&cached.store, &cached.dir, &SEAL)
        .unwrap()
        .unwrap();
    assert_eq!(frame.pixels, [1, 2, 3, 255]);
}

#[test]
fn automatic_download_respects_strangers_metering_and_explicit_off() {
    let f = Fixture::new(true, true);
    let snapshot = f.select();
    f.store
        .set_auto_download(attachment::AutoDownload::ByNetwork)
        .unwrap();
    assert!(snapshot.auto_download(&f.store, false).unwrap());
    assert!(!snapshot.auto_download(&f.store, true).unwrap());
    f.store
        .set_auto_download(attachment::AutoDownload::OnTap)
        .unwrap();
    assert!(!snapshot.auto_download(&f.store, false).unwrap());
    let stranger = Fixture::new(false, true);
    stranger
        .store
        .set_auto_download(attachment::AutoDownload::Always)
        .unwrap();
    assert!(!stranger
        .select()
        .auto_download(&stranger.store, false)
        .unwrap());
}

#[test]
fn failed_cache_retention_does_not_discard_validated_display_pixels() {
    let f = Fixture::new(true, true);
    std::fs::write(f.dir.join("originals"), b"unusable cache fixture").unwrap();
    let frame = f
        .select()
        .finish(&f.store, &f.dir, &SEAL, &f.bytes)
        .unwrap();
    assert_eq!(frame.pixels, [1, 2, 3, 255]);
    assert_eq!(
        std::fs::read(f.dir.join("originals")).unwrap(),
        b"unusable cache fixture"
    );
}
