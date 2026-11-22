//! Actual native readback over durable received interactions, without a node or published profile.
use super::favorites::Fixture;
use super::*;
use dyapp_client::interactions::{Interaction, Kind};
use dyapp_messaging::hlc::Timestamp;

#[test]
fn native_received_views_are_read_only_private_and_survive_reopening() {
    const NOW: u64 = 1_700_000_000_000;
    let f = Fixture::new();
    let peer = IdentityKey::generate().unwrap().peer_id();
    let mut store = Store::open(
        &std::path::Path::new(&f.dir).join("client.db"),
        &f.key.identity,
    )
    .unwrap();
    assert!(!store.send_profile_views().unwrap());
    assert!(!store
        .receive_interaction(
            &peer,
            b"peer device",
            Interaction {
                kind: Kind::View,
                event_id: [1; 16],
                hlc: Timestamp::new(NOW, 0).into()
            },
            NOW,
        )
        .unwrap());
    let expected = vec![ReceivedProfileView {
        peer_id: peer.clone(),
        utc_date: "2023-11-14".into(),
    }];
    assert_eq!(f.chats.received_profile_views().unwrap(), expected);
    assert_eq!(f.chats.received_profile_views().unwrap(), expected);
    assert_eq!(store.due_outbound(i64::MAX).unwrap().len(), 0);
    assert!(store.chat_rows().unwrap().is_empty());
    assert!(!store.likes(&peer).unwrap().matched());
    let other = Fixture::new();
    assert_eq!(other.chats.received_profile_views().unwrap(), []);
    drop(store);
    let reopened = f.reopen();
    assert_eq!(reopened.chats.received_profile_views().unwrap(), expected);
    let store = Store::open(
        &std::path::Path::new(&reopened.dir).join("client.db"),
        &reopened.key.identity,
    )
    .unwrap();
    store.set_blocked(&peer, true).unwrap();
    assert_eq!(reopened.chats.received_profile_views().unwrap(), []);
}
