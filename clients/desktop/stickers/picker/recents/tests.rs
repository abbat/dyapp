use super::*;
use std::cell::Cell;

fn fixture() -> (Vec<u8>, Vec<RecentSticker>) {
    let bytes = super::super::super::native_tests::archive_labels(&[
        "😺", "🐱", "😸", "😹", "😻", "😼", "😽", "🙀", "😿",
    ]);
    let pack = dyapp_client::sticker_pack::import_tar(&bytes).unwrap();
    let recent = pack
        .stickers()
        .iter()
        .map(|entry| RecentSticker {
            pack_id: *pack.id(),
            sha256: entry.sha256,
            emoji: entry.emoji.clone(),
        })
        .collect();
    (bytes, recent)
}

#[test]
fn recent_pages_preserve_labels_and_bound_pixels_and_pack_reads() {
    let (bytes, recent) = fixture();
    let reads = Cell::new(0);
    let load = |_: &[u8; 32]| {
        reads.set(reads.get() + 1);
        Ok(Some(
            dyapp_client::sticker_pack::import_tar(&bytes).unwrap(),
        ))
    };
    let first = page(&recent, 0, load, || false).unwrap();
    assert_eq!(reads.get(), 1);
    assert_eq!(first.entries.len(), 8);
    assert_eq!(first.next, Some(8));
    for (index, entry) in first.entries.iter().enumerate() {
        assert_eq!(entry.sticker.index, index);
        assert_eq!(entry.sticker.emoji, recent[index].emoji);
        assert_eq!(entry.pack_name, "Animals");
        assert!((1..=96).contains(&entry.sticker.frame.width));
        assert!((1..=96).contains(&entry.sticker.frame.height));
        assert_eq!(
            entry.sticker.frame.pixels.len(),
            (entry.sticker.frame.width * entry.sticker.frame.height * 4) as usize
        );
    }
    let json = serde_json::to_value(&first.entries[0]).unwrap();
    let mut keys: Vec<_> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["emoji", "frame", "index", "packId", "packName"]);
    let second = page(&recent, first.next.unwrap(), load, || false).unwrap();
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.entries[0].sticker.index, 8);
    assert_eq!(second.next, None);
    assert!(page(&recent, recent.len(), load, || false)
        .unwrap()
        .entries
        .is_empty());
    assert!(page(&recent, recent.len() + 1, load, || false).is_err());
}

#[test]
fn missing_packs_and_unknown_labels_do_not_make_broken_choices() {
    let (bytes, mut recent) = fixture();
    let mut unknown = RecentSticker {
        pack_id: recent[0].pack_id,
        sha256: recent[0].sha256,
        emoji: "🚀".into(),
    };
    recent.insert(0, unknown.clone());
    unknown.pack_id = [9; 32];
    recent.insert(0, unknown);
    let load = |id: &[u8; 32]| {
        Ok(if id == &[9; 32] {
            None
        } else {
            Some(dyapp_client::sticker_pack::import_tar(&bytes).unwrap())
        })
    };
    let first = page(&recent, 0, load, || false).unwrap();
    assert_eq!(first.entries.len(), 8);
    assert_eq!(first.next, Some(10));
    let last = page(&recent, 10, load, || false).unwrap();
    assert_eq!(last.entries.len(), 1);
    assert_eq!(last.entries[0].sticker.index, 8);
    assert_eq!(last.next, None);
}

#[test]
fn cancellation_and_storage_errors_never_publish_partial_pages() {
    let (bytes, recent) = fixture();
    assert!(page(&recent, 0, |_| panic!("cancelled before reading"), || true).is_err());
    assert!(page(&recent, 0, |_| Err("storage unavailable".into()), || false).is_err());
    let stopped = Cell::new(false);
    assert!(page(
        &recent,
        0,
        |_| {
            stopped.set(true);
            Ok(Some(
                dyapp_client::sticker_pack::import_tar(&bytes).unwrap(),
            ))
        },
        || stopped.get()
    )
    .is_err());
    let mut wrong = recent;
    wrong[0].pack_id = [8; 32];
    assert!(page(
        &wrong,
        0,
        |_| {
            Ok(Some(
                dyapp_client::sticker_pack::import_tar(&bytes).unwrap(),
            ))
        },
        || false
    )
    .is_err());
}
