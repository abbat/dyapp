use super::favorites::Fixture;
use super::offline::snapshot;
use super::saved_sticker::save;
use dyapp_client::{sticker_pack::Pack, sticker_picker as core};
use prost::Message;
use std::path::Path;

fn pack() -> Pack {
    #[derive(Clone, PartialEq, Message)]
    struct Record {
        #[prost(bytes = "vec", tag = "1")]
        manifest: Vec<u8>,
        #[prost(bytes = "vec", repeated, tag = "2")]
        files: Vec<Vec<u8>>,
    }
    let image = Record::decode(super::saved_sticker::pack().encode().as_slice())
        .unwrap()
        .files
        .remove(0);
    let labels = ["😺", "🐱", "😸", "😹", "😻", "😼", "😽", "🙀", "😿"];
    let hash = dyapp_identity::hex(&dyapp_identity::sha256(&image));
    let entries = labels
        .iter()
        .enumerate()
        .map(|(index, emoji)| {
            format!(r#"{{"file":"{index}.webp","emoji":"{emoji}","sha256":"{hash}"}}"#)
        })
        .collect::<Vec<_>>()
        .join(",");
    let manifest =
        format!(r#"{{"format":1,"name":"Picker cats","stickers":[{entries}]}}"#).into_bytes();
    let id = dyapp_identity::sha256(&manifest);
    dyapp_client::sticker_pack::from_peer(
        &Record {
            manifest,
            files: vec![image; labels.len()],
        }
        .encode_to_vec(),
        &id,
    )
    .unwrap()
}

fn assert_choice(actual: &crate::StickerPickerEntry, expected: &core::Choice) {
    assert_eq!(actual.index, u32::try_from(expected.index).unwrap());
    assert_eq!(actual.emoji, expected.emoji);
    assert_eq!(
        (actual.width, actual.height),
        (expected.frame.width, expected.frame.height)
    );
    assert_eq!(actual.pixels, expected.frame.pixels);
    assert!((1..=96).contains(&actual.width) && (1..=96).contains(&actual.height));
}

#[test]
fn saved_picker_exports_bounded_manifest_pages_and_exact_core_pixels_without_writes() {
    let f = Fixture::new();
    let pack = pack();
    save(&f, &pack, &f.chats.seal_key);
    let before = snapshot(&f);
    let first = f.chats.saved_sticker_page(pack.id().to_vec(), 0).unwrap();
    assert_eq!(first.pack_id, pack.id());
    assert_eq!(first.name, "Picker cats");
    assert_eq!(first.total, 9);
    assert_eq!(first.next, Some(8));
    assert_eq!(first.entries.len(), 8);
    for (actual, expected) in first
        .entries
        .iter()
        .zip(core::page(&pack, 0).unwrap().entries)
    {
        assert_choice(actual, &expected);
    }
    let last = f
        .chats
        .saved_sticker_page(pack.id().to_vec(), first.next.unwrap())
        .unwrap();
    assert_eq!(last.entries.len(), 1);
    assert_eq!(last.entries[0].index, 8);
    assert_eq!(last.next, None);
    assert_eq!(
        f.chats
            .saved_sticker_page(pack.id().to_vec(), 9)
            .unwrap()
            .entries,
        Vec::new()
    );
    for after in [10, u32::MAX] {
        assert_eq!(
            f.chats
                .saved_sticker_page(pack.id().to_vec(), after)
                .unwrap_err()
                .to_string(),
            "invalidIndex"
        );
    }
    assert_eq!(snapshot(&f), before);
}

fn recents(f: &Fixture, pack: &Pack) {
    let peer = super::offline::establish(f);
    let mut session = f.chats.session.lock().unwrap();
    let group = session
        .store()
        .known_groups()
        .unwrap()
        .into_iter()
        .find(|(_, partner)| partner == &peer)
        .unwrap()
        .0;
    let mut stage = |id, hash, emoji: &str| {
        session
            .stage_sticker(
                &group,
                &dyapp_client::chat::sticker::Sticker {
                    id: [0; 16],
                    hlc: 0,
                    pack_id: id,
                    sha256: hash,
                    emoji: emoji.into(),
                    pack_name: pack.name().into(),
                    attachment: None,
                },
                &f.chats.seal_key,
            )
            .unwrap();
    };
    // Successful local MLS/outbox staging, not direct SQL or network delivery, records recents.
    for sticker in pack.stickers().iter().rev() {
        stage(*pack.id(), sticker.sha256, &sticker.emoji);
    }
    stage(*pack.id(), pack.stickers()[0].sha256, "🚀");
    stage([9; 32], pack.stickers()[0].sha256, "😺");
}

#[test]
fn recent_picker_keeps_exact_labels_original_cursor_and_metadata_without_writes() {
    let f = Fixture::new();
    let pack = pack();
    save(&f, &pack, &f.chats.seal_key);
    recents(&f, &pack);
    let before = snapshot(&f);
    let first = f.chats.recent_sticker_page(0).unwrap();
    assert_eq!(first.entries.len(), 8);
    assert_eq!(first.next, Some(10));
    for (index, entry) in first.entries.iter().enumerate() {
        assert_eq!(entry.pack_id, pack.id());
        assert_eq!(entry.pack_name, pack.name());
        assert_choice(&entry.sticker, &core::thumbnail(&pack, index).unwrap());
    }
    let last = f.chats.recent_sticker_page(first.next.unwrap()).unwrap();
    assert_eq!(last.entries.len(), 1);
    assert_eq!(last.entries[0].sticker.index, 8);
    assert_eq!(last.next, None);
    assert_eq!(f.chats.recent_sticker_page(11).unwrap().entries, Vec::new());
    assert_eq!(
        f.chats
            .recent_sticker_page(u32::MAX)
            .unwrap_err()
            .to_string(),
        "invalidIndex"
    );
    assert_eq!(
        f.chats
            .session
            .lock()
            .unwrap()
            .store()
            .recent_stickers()
            .unwrap()
            .len(),
        11
    );
    assert_eq!(snapshot(&f), before);
}

#[test]
fn picker_refuses_malformed_absent_and_foreign_content_without_removing_files() {
    let f = Fixture::new();
    let other = Fixture::new();
    let pack = pack();
    let before = snapshot(&f);
    assert_eq!(
        f.chats
            .saved_sticker_page(vec![0; 31], 0)
            .unwrap_err()
            .to_string(),
        "invalidPackId"
    );
    assert_eq!(
        f.chats
            .saved_sticker_page(pack.id().to_vec(), 0)
            .unwrap_err()
            .to_string(),
        "notFound"
    );
    assert_eq!(f.chats.recent_sticker_page(0).unwrap().entries, Vec::new());
    assert_eq!(snapshot(&f), before);
    save(&other, &pack, &f.chats.seal_key);
    let name = dyapp_identity::hex(pack.id());
    let source = Path::new(&other.dir).join("sticker-packs").join(&name);
    let dir = Path::new(&f.dir).join("sticker-packs");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join(name);
    std::fs::copy(source, &target).unwrap();
    let bytes = std::fs::read(&target).unwrap();
    recents(&f, &pack);
    let before = snapshot(&f);
    assert!(f.chats.saved_sticker_page(pack.id().to_vec(), 0).is_err());
    assert_eq!(f.chats.recent_sticker_page(0).unwrap().entries, Vec::new());
    assert_eq!(std::fs::read(target).unwrap(), bytes);
    assert_eq!(snapshot(&f), before);
}
