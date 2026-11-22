//! Exercise public operation boundaries in the package cargo-mutants actually tests.
use super::tests::{bob_invited, Invite};
use super::Error;
use crate::store::InboxKey;
use dyapp_identity::Identity;
use dyapp_p2p_net::{Mode, SwarmOptions};

fn fixture(seal: &[u8; 32]) -> (dyapp_test_support::TestDir, Invite) {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let invite = bob_invited(&test_dir.path().join("client-1.db"), seal);
    (test_dir, invite)
}

#[tokio::test]
async fn last_resort_rotation_overflow_cannot_advance_snapshot_or_publish_a_record() {
    let seal = [5; 32];
    let (_test_dir, mut f) = fixture(&seal);
    let before = f.session.store().sealed_device().unwrap();
    let options = SwarmOptions {
        mode: Mode::Client,
        ..Default::default()
    };
    let mut node = crate::Node {
        swarm: dyapp_p2p_net::build_configured_swarm(
            libp2p::identity::Keypair::generate_ed25519(),
            options,
        )
        .unwrap(),
        peer: libp2p::PeerId::random(),
        address: libp2p::Multiaddr::empty(),
    };
    assert!(matches!(
        f.session
            .keep_last_resort(
                &mut node,
                &Identity::try_generate().unwrap(),
                u64::MAX,
                &seal
            )
            .await,
        Err(Error::Store(crate::store::Error::ClockRange))
    ));
    assert_eq!(f.session.store().sealed_device().unwrap(), before);
    assert!(f.session.store().last_resort().unwrap().is_none());
    assert!(f.session.due_outbound(i64::MAX).unwrap().is_empty());
}

fn accept_match(f: &mut Invite, seal: &[u8; 32]) {
    f.session
        .stage_envelope(
            &InboxKey {
                mailbox: [9; 32],
                id: [1; 16],
            },
            &f.welcome,
            &f.profiles,
            seal,
        )
        .unwrap();
    let (alice, bob) = (f.alice_id.peer_id(), f.bob_id.peer_id());
    for (sender, target, id) in [(&alice, &bob, 1), (&bob, &alice, 2)] {
        f.session
            .store
            .apply_interaction(
                sender,
                target,
                b"d",
                crate::interactions::Interaction {
                    kind: crate::interactions::Kind::Like,
                    event_id: [id; 16],
                    hlc: 1,
                },
            )
            .unwrap();
    }
}

#[tokio::test]
async fn public_presence_commits_retryable_hidden_work_without_a_delivery_node() {
    let seal = [6; 32];
    let (_test_dir, mut f) = fixture(&seal);
    accept_match(&mut f, &seal);
    assert!(f.session.due_outbound(i64::MAX).unwrap().is_empty());
    assert!(matches!(
        f.session.announce_presence(&mut [], &seal).await,
        Err(Error::Client(crate::Error::Status(
            dyapp_p2p_net::proto::Status::NotFound
        )))
    ));
    let writes = f.session.due_outbound(i64::MAX).unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0].envelope().class,
        dyapp_p2p_net::proto::EnvelopeClass::Signal as i32,
    );
    assert!(f
        .session
        .store()
        .current_chat_messages(&f.alice_id.peer_id())
        .unwrap()
        .is_empty());
}
