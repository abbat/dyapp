//! Offline collection pages preserve core ordering, accounting and unreadable files.
use super::favorites::Fixture;
use super::saved_sticker::{pack, save};
use crate::{SavedStickerPackEntry, SavedStickerPackPage, SavedStickerPackState};
use std::path::{Path, PathBuf};

const UNREADABLE: &[u8] = b"untrusted pack name";
const PENDING: &[u8] = b"pending";

fn directory(f: &Fixture) -> PathBuf {
    Path::new(&f.dir).join("sticker-packs")
}

fn page(f: &Fixture, after: Option<Vec<u8>>) -> SavedStickerPackPage {
    f.chats.saved_sticker_packs(after).unwrap()
}

#[test]
fn empty_collection_is_offline_and_invalid_cursor_does_not_create_storage() {
    let f = Fixture::new();
    let empty = page(&f, None);
    assert_eq!(empty.entries, []);
    assert!(empty.next.is_none());
    assert_eq!((empty.used_bytes, empty.unrecognized_files), (0, 0));
    for cursor in [vec![], vec![0; 31], vec![0; 33]] {
        assert_eq!(
            f.chats
                .saved_sticker_packs(Some(cursor))
                .unwrap_err()
                .to_string(),
            "invalidCursor"
        );
    }
    assert!(!directory(&f).exists());
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

fn populate(f: &Fixture) -> (Vec<Vec<u8>>, u64) {
    let pack = pack();
    save(f, &pack, &f.chats.seal_key);
    let dir = directory(f);
    let mut ids = vec![pack.id().to_vec()];
    let mut used = std::fs::metadata(dir.join(dyapp_identity::hex(pack.id())))
        .unwrap()
        .len();
    for byte in 0..33 {
        let id = vec![byte; 32];
        if id == pack.id() {
            continue;
        }
        std::fs::write(dir.join(dyapp_identity::hex(&id)), UNREADABLE).unwrap();
        used += u64::try_from(UNREADABLE.len()).unwrap();
        ids.push(id);
    }
    std::fs::write(dir.join(".interrupted.pending"), PENDING).unwrap();
    ids.sort();
    (ids, used + u64::try_from(PENDING.len()).unwrap())
}

#[test]
fn collection_pages_preserve_core_cursors_authenticated_labels_and_global_accounting() {
    let f = Fixture::new();
    let (ids, used) = populate(&f);
    let first = page(&f, None);
    assert_eq!(first.entries.len(), 32);
    assert_eq!(first.next.as_ref(), Some(&ids[31]));
    let second = page(&f, first.next.clone());
    assert!(second.next.is_none());
    for current in [&first, &second] {
        assert_eq!((current.used_bytes, current.unrecognized_files), (used, 1));
    }
    let entries: Vec<_> = first.entries.into_iter().chain(second.entries).collect();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.pack_id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    check_entries(&f, entries);
    assert_eq!(
        std::fs::read(directory(&f).join(".interrupted.pending")).unwrap(),
        PENDING
    );
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

fn check_entries(f: &Fixture, entries: Vec<SavedStickerPackEntry>) {
    let mut readable = 0;
    for entry in entries {
        let path = directory(f).join(dyapp_identity::hex(&entry.pack_id));
        assert_eq!(entry.bytes, std::fs::metadata(&path).unwrap().len());
        match entry.state {
            SavedStickerPackState::Ready { name, stickers } => {
                readable += 1;
                assert_eq!((name.as_str(), stickers), ("Cats", 1));
            }
            SavedStickerPackState::Unreadable { detail } => {
                assert_ne!(detail, "");
                assert_eq!(std::fs::read(path).unwrap(), UNREADABLE);
            }
        }
    }
    assert_eq!(readable, 1);
}

#[test]
fn collection_does_not_trust_another_owner_even_with_the_same_sealing_key() {
    let f = Fixture::new();
    let other = Fixture::new();
    let pack = pack();
    save(&other, &pack, &f.chats.seal_key);
    let name = dyapp_identity::hex(pack.id());
    std::fs::create_dir_all(directory(&f)).unwrap();
    let source = directory(&other).join(&name);
    let target = directory(&f).join(name);
    std::fs::copy(&source, &target).unwrap();
    let entries = page(&f, None).entries;
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        &entries[0].state,
        SavedStickerPackState::Unreadable { .. }
    ));
    assert_eq!(
        std::fs::read(source).unwrap(),
        std::fs::read(target).unwrap()
    );
}
