//! Native frames retain authenticated selectors and core pixels without file/network mutations.
use super::favorites::Fixture;
use super::saved_sticker::{pack, save};
use std::path::{Path, PathBuf};

fn path(f: &Fixture, id: &[u8; 32]) -> PathBuf {
    Path::new(&f.dir)
        .join("sticker-packs")
        .join(dyapp_identity::hex(id))
}

fn no_outbox(f: &Fixture) {
    assert!(f
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .due_outbound(i64::MAX)
        .unwrap()
        .is_empty());
}

#[test]
fn saved_frame_retains_authenticated_selection_and_exact_core_rgba() {
    let f = Fixture::new();
    let pack = pack();
    save(&f, &pack, &f.chats.seal_key);
    let file = path(&f, pack.id());
    let sealed = std::fs::read(&file).unwrap();
    let expected =
        dyapp_client::sticker_webp::decode::first_frame(pack.sticker_bytes(0).unwrap()).unwrap();
    let frame = f.chats.saved_sticker_frame(pack.id().to_vec(), 0).unwrap();
    assert_eq!(frame.pack_id, pack.id());
    assert_eq!(frame.index, 0);
    assert_eq!(frame.emoji, "😺");
    assert_eq!(frame.sha256, pack.stickers()[0].sha256);
    assert_eq!(
        (frame.width, frame.height),
        (expected.width, expected.height)
    );
    assert_eq!(frame.pixels, expected.pixels);
    assert_eq!(
        frame.pixels.len(),
        usize::try_from(frame.width * frame.height * 4).unwrap()
    );
    assert_eq!(std::fs::read(file).unwrap(), sealed);
    no_outbox(&f);
}

#[test]
fn frame_selection_errors_are_offline_and_preserve_saved_content() {
    let f = Fixture::new();
    let read = |id, index| {
        f.chats
            .saved_sticker_frame(id, index)
            .unwrap_err()
            .to_string()
    };
    assert_eq!(read(vec![0; 31], 0), "invalidPackId");
    assert_eq!(read(vec![0; 32], 0), "notFound");
    let pack = pack();
    save(&f, &pack, &f.chats.seal_key);
    let file = path(&f, pack.id());
    let sealed = std::fs::read(&file).unwrap();
    assert_eq!(read(pack.id().to_vec(), 1), "invalidIndex");
    assert_eq!(read(pack.id().to_vec(), u32::MAX), "invalidIndex");
    assert_eq!(std::fs::read(&file).unwrap(), sealed);
    std::fs::write(&file, b"corrupt saved fixture").unwrap();
    assert!(f.chats.saved_sticker_frame(pack.id().to_vec(), 0).is_err());
    assert_eq!(std::fs::read(file).unwrap(), b"corrupt saved fixture");
    no_outbox(&f);
}

#[test]
fn saved_frame_refuses_foreign_owners_and_keys_without_removing_the_file() {
    let f = Fixture::new();
    let other = Fixture::new();
    let pack = pack();
    save(&other, &pack, &f.chats.seal_key);
    let source = path(&other, pack.id());
    let target = path(&f, pack.id());
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::copy(&source, &target).unwrap();
    let foreign_owner = std::fs::read(&target).unwrap();
    assert!(f.chats.saved_sticker_frame(pack.id().to_vec(), 0).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), foreign_owner);
    let own_pack = pack.id();
    // A separate identity-local file, encrypted with the wrong secret, also cannot be displayed.
    let keyed = Fixture::new();
    let mut wrong_key = *keyed.chats.seal_key;
    wrong_key[0] ^= 1;
    save(&keyed, &pack, &wrong_key);
    assert!(keyed
        .chats
        .saved_sticker_frame(own_pack.to_vec(), 0)
        .is_err());
    assert!(path(&keyed, own_pack).exists());
    no_outbox(&f);
    no_outbox(&keyed);
}
