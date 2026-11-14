//! Public saved-pack submission preflight is offline and cannot create outbox entries.
use super::favorites::Fixture;
use super::*;
use dyapp_client::sticker_pack::{self, Pack};
use prost::Message as _;
use std::path::Path;

pub(super) fn pack() -> Pack {
    #[derive(Clone, PartialEq, prost::Message)]
    struct Record {
        #[prost(bytes = "vec", tag = "1")]
        manifest: Vec<u8>,
        #[prost(bytes = "vec", repeated, tag = "2")]
        files: Vec<Vec<u8>>,
    }
    let image = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fuzz/corpus/sticker_webp/lossless-alpha.webp"
    ));
    let hash = dyapp_identity::hex(&dyapp_identity::sha256(image));
    let manifest = format!(
        "{{\"format\":1,\"name\":\"Cats\",\"stickers\":[{{\"file\":\"cat.webp\",\"emoji\":\"😺\",\"sha256\":\"{hash}\"}}]}}"
    ).into_bytes();
    let id = dyapp_identity::sha256(&manifest);
    sticker_pack::from_peer(
        &Record {
            manifest,
            files: vec![image.to_vec()],
        }
        .encode_to_vec(),
        &id,
    )
    .unwrap()
}

pub(super) fn save(f: &Fixture, pack: &Pack, secret: &[u8; 32]) {
    let mut store = Store::open(&Path::new(&f.dir).join("client.db"), &f.key.identity).unwrap();
    store.save_sticker_pack(secret, pack).unwrap();
}

fn no_message(f: &Fixture, peer: &str) {
    let session = f.chats.session.lock().unwrap();
    assert!(session.store().due_outbound(i64::MAX).unwrap().is_empty());
    assert!(session
        .store()
        .current_chat_stickers(peer)
        .unwrap()
        .is_empty());
    assert_eq!(
        session.store().conversation(peer).unwrap(),
        Conversation::None
    );
}

#[test]
fn invalid_saved_selection_never_uses_network_or_stages() {
    let f = Fixture::new();
    let peer = "ab".repeat(32);
    let submit = |target, id, index| {
        f.chats
            .submit_saved_sticker(target, id, index)
            .unwrap_err()
            .to_string()
    };
    assert_eq!(submit(peer.clone(), vec![1; 31], 0), "invalidPackId");
    assert_eq!(submit(f.key.peer_id(), vec![1; 32], 0), "ownPeer");
    assert_eq!(submit(peer.clone(), vec![1; 32], 0), "notFound");
    let pack = pack();
    save(&f, &pack, &f.chats.seal_key);
    assert_eq!(submit(peer.clone(), pack.id().to_vec(), 1), "invalidIndex");
    assert_eq!(
        submit(peer.clone(), pack.id().to_vec(), u32::MAX),
        "invalidIndex"
    );
    assert_eq!(submit(peer.clone(), pack.id().to_vec(), 0), "unpublished");
    no_message(&f, &peer);
}

#[test]
fn same_sealing_key_cannot_authorize_another_identitys_saved_pack() {
    let f = Fixture::new();
    let other = Fixture::new();
    let pack = pack();
    save(&other, &pack, &f.chats.seal_key);
    let name = dyapp_identity::hex(pack.id());
    let source = Path::new(&other.dir).join("sticker-packs").join(&name);
    let target_dir = Path::new(&f.dir).join("sticker-packs");
    std::fs::create_dir_all(&target_dir).unwrap();
    let target = target_dir.join(name);
    std::fs::copy(&source, &target).unwrap();
    let peer = "ab".repeat(32);
    assert!(f
        .chats
        .submit_saved_sticker(peer.clone(), pack.id().to_vec(), 0)
        .is_err());
    assert_eq!(
        std::fs::read(target).unwrap(),
        std::fs::read(source).unwrap()
    );
    no_message(&f, &peer);
}

#[test]
fn foreign_sealing_key_and_corrupt_pack_are_preserved_without_submission() {
    let f = Fixture::new();
    let peer = "ab".repeat(32);
    let pack = pack();
    let mut foreign = *f.chats.seal_key;
    foreign[0] ^= 1;
    save(&f, &pack, &foreign);
    let path = Path::new(&f.dir)
        .join("sticker-packs")
        .join(dyapp_identity::hex(pack.id()));
    let sealed = std::fs::read(&path).unwrap();
    assert!(f
        .chats
        .submit_saved_sticker(peer.clone(), pack.id().to_vec(), 0)
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), sealed);
    std::fs::write(&path, b"corrupt pack fixture").unwrap();
    assert!(f
        .chats
        .submit_saved_sticker(peer.clone(), pack.id().to_vec(), 0)
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt pack fixture");
    no_message(&f, &peer);
}
