use super::*;
use dyapp_client::interactions::{Interaction, Kind};
use dyapp_identity::Identity;

fn assert_page_navigation(store: &Store, peers: &[String]) -> core::Cursor {
    let first = page(store, "", None).unwrap();
    assert_eq!(first.total, peers.len());
    assert_eq!(first.entries.len(), core::PAGE_SIZE);
    assert_eq!(first.entries[0].target, peers[0]);
    assert!(first
        .entries
        .iter()
        .all(|entry| entry.label == "2023-11-14"));
    assert!(first.previous.is_none());
    let next: core::Cursor = first.next.unwrap().into();
    let last = page(store, "", Some(&next)).unwrap();
    assert_eq!(last.entries.len(), 1);
    assert_eq!(last.entries[0].target, peers[core::PAGE_SIZE]);
    assert!(last.next.is_none());
    assert!(last.previous.is_some());
    next
}

#[test]
fn received_pages_preserve_core_order_dates_targets_and_query_bound_navigation() {
    const NOW: u64 = 1_700_000_000_000;
    let owner = Identity::try_generate().unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build")
        .join(format!("desktop-view-pages-{}", owner.peer_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = crate::chats::open(&dir, &owner).unwrap();
    let mut peers: Vec<_> = (0..=core::PAGE_SIZE)
        .map(|_| Identity::try_generate().unwrap().peer_id())
        .collect();
    peers.sort();
    // Core drafting supplies the wire clock without committing an outgoing interaction.
    let clock = store
        .draft_interaction(&peers[0], Kind::Unlike, NOW)
        .unwrap()
        .unwrap()
        .hlc;
    for peer in &peers {
        store
            .receive_interaction(
                peer,
                b"peer device",
                Interaction {
                    kind: Kind::View,
                    event_id: [1; 16],
                    hlc: clock,
                },
                NOW,
            )
            .unwrap();
    }
    assert!(!store.send_profile_views().unwrap());
    let next = assert_page_navigation(&store, &peers);
    let found = page(&store, &peers[0], Some(&next)).unwrap();
    assert_eq!(found.number, 1);
    assert_eq!(found.matched, 1);
    assert_eq!(found.entries[0].target, peers[0]);
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
    assert!(store.chat_rows().unwrap().is_empty());
}
