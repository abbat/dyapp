use super::*;
use crate::interactions::{Interaction, Kind};
use dyapp_identity::Identity;
use dyapp_messaging::hlc::Timestamp;
use std::{fs, path::Path};

const T: u64 = 1_800_000_000_000;

fn fixture() -> (Store, String, String) {
    let owner = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/test-presence-projection")
        .join(owner.peer_id());
    fs::create_dir_all(&dir).unwrap();
    (
        Store::open(&dir.join("state.sqlite"), &owner).unwrap(),
        owner.peer_id(),
        peer,
    )
}

fn like(store: &mut Store, sender: &str, target: &str, id: u8) {
    store
        .apply_interaction(
            sender,
            target,
            b"device",
            Interaction {
                kind: Kind::Like,
                event_id: [id; 16],
                hlc: u64::from(id),
            },
        )
        .unwrap();
}

#[test]
fn preflight_is_read_only_and_reuses_match_hour_and_audience_rules() {
    let (mut store, owner, peer) = fixture();
    assert!(!due(&store).unwrap());
    like(&mut store, &owner, &peer, 1);
    assert!(!due_at(&store, T).unwrap());
    like(&mut store, &peer, &owner, 2);
    let drafted = store.draft_presence(&peer, T).unwrap().unwrap();
    assert!(due_at(&store, T).unwrap());
    assert!(due_at(&store, T).unwrap());
    assert_eq!(store.draft_presence(&peer, T).unwrap(), Some(drafted));
    let write = crate::prepare_put(&[7; 32], vec![5]).unwrap();
    store
        .stage_presence(&peer, drafted, b"sealed", &[write])
        .unwrap();
    assert!(!due_at(&store, T).unwrap());
    assert!(!due_at(&store, T + 3_599_999).unwrap());
    assert!(due_at(&store, T + 3_600_000).unwrap());
    store.set_send_presence(false).unwrap();
    assert!(!due_at(&store, T + 3_600_000).unwrap());
}

#[test]
fn incoming_projection_expires_and_is_not_reciprocal_with_the_local_send_setting() {
    let (mut store, owner, peer) = fixture();
    assert_eq!(active_peers_at(&store, T).unwrap().len(), 0);
    like(&mut store, &owner, &peer, 1);
    like(&mut store, &peer, &owner, 2);
    let received = Timestamp::new(T, 0).into();
    assert!(store.receive_presence(&peer, received, T).unwrap());
    assert_eq!(active_peers_at(&store, T).unwrap(), vec![peer.clone()]);
    store.set_send_presence(false).unwrap();
    assert_eq!(
        active_peers_at(&store, T + 7_200_000).unwrap(),
        vec![peer.clone()]
    );
    assert_eq!(active_peers_at(&store, T + 7_200_001).unwrap().len(), 0);
    store
        .apply_interaction(
            &owner,
            &peer,
            b"device",
            Interaction {
                kind: Kind::Unlike,
                event_id: [3; 16],
                hlc: Timestamp::new(T + 1, 0).into(),
            },
        )
        .unwrap();
    assert_eq!(active_peers_at(&store, T + 1).unwrap().len(), 0);
}
