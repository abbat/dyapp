//! Exact pending-link retries must retain later local and synchronized private choices.
#[path = "retry_tests/progress.rs"]
mod progress_cases;
#[path = "retry_tests/review.rs"]
mod review_cases;
use super::super::tests::temp_db_path;
use super::*;
use crate::{chat::PeerChange, interactions::Kind};
use dyapp_identity::Identity;
use prost::Message as _;

fn private_choices(store: &Store, on: bool, at: u64) {
    let name = if on { "original alias" } else { "later alias" };
    let note = if on { "original note" } else { "" };
    for change in [
        PeerChange::Blocked(on),
        PeerChange::Muted(on),
        PeerChange::Favorite(on),
        PeerChange::Name(name.into()),
        PeerChange::Note(note.into()),
        PeerChange::Passed(on),
    ] {
        store
            .apply_peer_change(
                "carol",
                &change,
                at.into(),
                if on { b"phone" } else { b"laptop" },
            )
            .unwrap();
    }
}

fn source(identity: &Identity) -> Vec<u8> {
    let mut store = Store::open(&temp_db_path(), identity).unwrap();
    private_choices(&store, true, 1_000);
    store.interact("dave", Kind::Like, b"phone", 1_000).unwrap();
    store.set_profile_hidden(true).unwrap();
    store.link_state().unwrap()
}

#[test]
fn pending_prefix_retry_keeps_newer_private_state_after_reopen() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let state = source(&identity);
    let mut store = Store::open(&path, &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    store.import_link_state(&state).unwrap();
    private_choices(&store, false, 2_000);
    store
        .interact("dave", Kind::Unlike, b"laptop", 2_000)
        .unwrap();
    store.set_profile_hidden(false).unwrap();
    let expected = LinkState::decode(store.link_state().unwrap().as_slice()).unwrap();
    drop(store);
    let mut store = Store::open(&path, &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    store.import_link_state(&state).unwrap();
    let actual = LinkState::decode(store.link_state().unwrap().as_slice()).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(store.link_pending().unwrap(), Some(state));
}
