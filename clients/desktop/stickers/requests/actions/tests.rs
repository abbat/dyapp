use super::*;
use dyapp_identity::Identity;

#[test]
fn unknown_blocked_and_own_peer_actions_cannot_create_intent_or_mls() {
    let owner = Identity::generate();
    let peer = Identity::generate().peer_id();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build")
        .join(format!("native-pack-actions-{}", owner.peer_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("client.db");
    for (target, blocked, reason) in [
        (peer.clone(), false, "unknownPeer"),
        (peer.clone(), true, "blocked"),
        (owner.peer_id(), false, "ownPeer"),
    ] {
        for kind in [Kind::Request, Kind::Retry, Kind::Fresh, Kind::StopWaiting] {
            let store = Store::open(&path, &owner).unwrap();
            store.set_blocked(&peer, blocked).unwrap();
            let selection = Selection {
                peer_id: target.clone(),
                group_id: vec![9],
                author: target.clone(),
                id: [1; 16],
            };
            assert_eq!(
                apply(
                    store,
                    &[7; 32],
                    &selection,
                    &Action {
                        kind,
                        generation: Some([3; 16])
                    }
                )
                .unwrap_err(),
                reason
            );
            let store = Store::open(&path, &owner).unwrap();
            assert!(store
                .pending_sticker_pack(&peer, &[1; 32])
                .unwrap()
                .is_none());
            assert!(store.sealed_device().unwrap().is_none());
            assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
            assert!(!store.has_sticker_packs().unwrap());
        }
    }
}
