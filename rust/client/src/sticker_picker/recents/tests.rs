use super::*;
use std::cell::Cell;

fn records(pack: &Pack) -> Vec<RecentSticker> {
    pack.stickers()
        .iter()
        .map(|entry| RecentSticker {
            pack_id: *pack.id(),
            sha256: entry.sha256,
            emoji: entry.emoji.clone(),
        })
        .collect()
}
fn fixture() -> Pack {
    super::super::tests::pack_labels(
        &["😺", "🐱", "😸", "😹", "😻", "😼", "😽", "🙀", "😿"],
        1,
        1,
    )
}
fn reopen(pack: &Pack) -> Pack {
    crate::sticker_pack::from_peer(&pack.encode(), pack.id()).unwrap()
}

fn assert_choices(entries: &[Entry], recent: &[RecentSticker]) {
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(entry.sticker.index, index);
        assert_eq!(entry.sticker.emoji, recent[index].emoji);
        assert_eq!(entry.pack_name, "Cats");
        assert_eq!(recent[index].sha256, recent[0].sha256);
    }
}

#[test]
fn pages_preserve_distinct_labels_for_one_hash_and_cache_only_the_current_pack() {
    let pack = fixture();
    let recent = records(&pack);
    let reads = Cell::new(0);
    let load = |_: &[u8; 32]| {
        reads.set(reads.get() + 1);
        Ok(Some(reopen(&pack)))
    };
    let first = resolve(&recent, 0, load, || false).unwrap();
    assert_eq!(reads.get(), 1);
    assert_eq!(first.entries.len(), 8);
    assert_eq!(first.next, Some(8));
    assert_choices(&first.entries, &recent);
    let last = resolve(&recent, first.next.unwrap(), load, || false).unwrap();
    assert_eq!(reads.get(), 2);
    assert_eq!(last.entries.len(), 1);
    assert_eq!(last.entries[0].sticker.index, 8);
    assert_eq!(last.next, None);
    assert!(resolve(&recent, recent.len(), load, || false)
        .unwrap()
        .entries
        .is_empty());
    assert!(matches!(
        resolve(&recent, usize::MAX, load, || false),
        Err(Error::Picker(super::super::Error::InvalidIndex))
    ));
}

#[test]
fn filtering_preserves_cursor_offsets_over_original_records() {
    let pack = fixture();
    let mut recent = records(&pack);
    let mut missing = recent[0].clone();
    missing.emoji = "🚀".into();
    recent.insert(0, missing.clone());
    missing.pack_id = [9; 32];
    recent.insert(0, missing);
    let load = |id: &[u8; 32]| Ok((id != &[9; 32]).then(|| reopen(&pack)));
    let first = resolve(&recent, 0, load, || false).unwrap();
    assert_eq!(first.entries.len(), 8);
    assert_eq!(first.next, Some(10));
    let last = resolve(&recent, 10, load, || false).unwrap();
    assert_eq!(last.entries.len(), 1);
    assert_eq!(last.entries[0].sticker.index, 8);
    assert_eq!(last.next, None);
}

#[test]
fn cancellation_and_loader_failure_do_not_return_partial_pages() {
    let pack = fixture();
    let recent = records(&pack);
    assert!(matches!(
        resolve(&recent, 0, |_| panic!("cancel before load"), || true),
        Err(Error::IdentityChanged)
    ));
    assert!(matches!(
        resolve(
            &recent,
            0,
            |_| Err(PackError::Io(std::io::Error::other("fixture")).into()),
            || false
        ),
        Err(Error::Pack(PackError::Io(_)))
    ));
    let other = super::super::tests::pack_labels(&["🐱"], 1, 1);
    let mut both = vec![recent[0].clone()];
    both.push(records(&other).remove(0));
    let stopped = Cell::new(false);
    let result = resolve(
        &both,
        0,
        |id| {
            if id == other.id() {
                stopped.set(true);
                Ok(Some(reopen(&other)))
            } else {
                Ok(Some(reopen(&pack)))
            }
        },
        || stopped.get(),
    );
    assert!(matches!(result, Err(Error::IdentityChanged)));
}

#[test]
fn wrong_pack_identity_is_corruption_not_an_available_recent_choice() {
    let pack = fixture();
    let mut recent = records(&pack);
    recent[0].pack_id = [8; 32];
    assert!(matches!(
        resolve(&recent, 0, |_| Ok(Some(reopen(&pack))), || false),
        Err(Error::Picker(super::super::Error::Corrupted))
    ));
}

#[test]
fn saved_file_filtering_keeps_unreadable_bytes_and_recent_metadata() {
    let owner = dyapp_identity::Identity::generate();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/core-picker-recents")
        .join(owner.peer_id());
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("client.db");
    let mut store = Store::open(&path, &owner).unwrap();
    let valid = fixture();
    let invalid = super::super::tests::pack_labels(&["🐱"], 1, 1);
    store.save_sticker_pack(&[5; 32], &valid).unwrap();
    store.save_sticker_pack(&[5; 32], &invalid).unwrap();
    let file = dir
        .join("sticker-packs")
        .join(dyapp_identity::hex(invalid.id()));
    std::fs::write(&file, b"unreadable recent fixture").unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    for (order, pack_id) in [(30, [9; 32]), (20, *invalid.id()), (10, *valid.id())] {
        db.execute(
            "INSERT INTO `sticker_recents` (`pack_id`, `sticker_hash`, `emoji`, `used_hlc`) VALUES (?, ?, ?, ?)",
            rusqlite::params![pack_id.as_slice(), valid.stickers()[0].sha256.as_slice(), &valid.stickers()[0].emoji, order],
        ).unwrap();
    }
    let before = store.recent_stickers().unwrap();
    let result = page(&store, &[5; 32], 0, || false).unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].pack_id, *valid.id());
    assert_eq!(result.next, None);
    assert!(store.recent_stickers().unwrap() == before);
    assert_eq!(std::fs::read(file).unwrap(), b"unreadable recent fixture");
}
