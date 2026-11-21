use super::*;

fn entries() -> Vec<Entry> {
    (0..70)
        .map(|index| Entry {
            target: format!("peer-{index:03}-complete-target"),
            label: if index % 2 == 0 { "Анна" } else { "Robin" }.to_owned(),
        })
        .collect()
}

#[test]
fn navigation_covers_every_original_target_once_and_clamps_after_removal() {
    let entries = entries();
    let first = page(&entries, "", None);
    assert_eq!((first.total, first.matched, first.offset), (70, 70, 0));
    assert_eq!((first.number, first.pages), (1, 3));
    assert_eq!(first.entries, entries[..32]);
    assert!(first.previous.is_none());
    let second = page(&entries, "", first.next.as_ref());
    assert_eq!(second.entries, entries[32..64]);
    let last = page(&entries, "", second.next.as_ref());
    assert_eq!(last.entries, entries[64..]);
    assert_eq!((last.number, last.pages), (3, 3));
    assert!(last.next.is_none());
    assert_eq!(page(&entries, "", last.previous.as_ref()), second);
    let clamped = page(&entries[..33], "", second.next.as_ref());
    assert_eq!(clamped.offset, 32);
    assert_eq!(clamped.entries, entries[32..33]);
    assert!(clamped.next.is_none());
    let huge = Cursor {
        query: String::new(),
        offset: usize::MAX,
    };
    assert_eq!(page(&entries, "", Some(&huge)), last);
}

#[test]
fn search_matches_full_raw_targets_and_unicode_labels_without_changing_them() {
    let entries = entries();
    let first = page(&entries, " \u{feff}АННА\n", None);
    assert_eq!((first.total, first.matched), (70, 35));
    assert_eq!(first.entries.len(), PAGE_SIZE);
    assert_eq!(first.entries.first(), entries.first());
    let last = page(&entries, "анна", first.next.as_ref());
    assert_eq!(last.entries.len(), 3);
    assert_eq!(last.entries.last(), entries.get(68));
    let changed = page(&entries, "Robin", first.next.as_ref());
    assert_eq!(changed.offset, 0);
    assert_eq!(changed.entries.first(), entries.get(1));
    let target = page(&entries, "PEER-069-COMPLETE-TARGET", None);
    assert_eq!(target.entries, entries[69..]);
    assert_eq!((target.total, target.matched), (70, 1));
    let missing = page(&entries, "not present", first.next.as_ref());
    assert_eq!((missing.total, missing.matched, missing.offset), (70, 0, 0));
    assert_eq!(missing.entries, Vec::<Entry>::new());
    assert!(missing.previous.is_none() && missing.next.is_none());
}

#[test]
fn empty_and_exact_page_boundaries_have_no_phantom_navigation() {
    let empty = page(&[], "", None);
    assert_eq!((empty.total, empty.matched, empty.offset), (0, 0, 0));
    assert_eq!((empty.number, empty.pages), (1, 1));
    assert_eq!(empty.entries, Vec::<Entry>::new());
    assert!(empty.previous.is_none() && empty.next.is_none());
    let entries = entries();
    let exact = page(&entries[..32], "", None);
    assert_eq!(exact.entries, entries[..32]);
    assert!(exact.previous.is_none() && exact.next.is_none());
    let unaligned = Cursor {
        query: String::new(),
        offset: 63,
    };
    let second = page(&entries, "", Some(&unaligned));
    assert_eq!(second.offset, 32);
    assert_eq!(second.entries, entries[32..64]);
}
