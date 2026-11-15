//! Real-node coverage of the common native profile-share operation.
use super::*;
use dyapp_ffi::ChatSubmission;

#[test]
fn native_profile_share_reaches_an_open_peer_as_one_ordinary_canonical_link() {
    let (_runtime, address) = store_node();
    let (alice_key, alice) = person(&address);
    let (bob_key, bob) = person(&address);
    let (alice_id, bob_id) = (alice_key.peer_id(), bob_key.peer_id());
    assert_eq!(
        alice
            .submit_profile_share(bob_id.clone(), alice_id.clone())
            .unwrap_err()
            .to_string(),
        "invalidTarget"
    );
    alice.send_text(bob_id.clone(), "hello".into()).unwrap();
    receive_events(&bob);
    bob.send_text(alice_id.clone(), "reply".into()).unwrap();
    receive_events(&alice);
    assert_eq!(alice.state(bob_id.clone()).unwrap(), ChatState::Open);
    let result = alice
        .submit_profile_share(bob_id.clone(), alice_id.clone())
        .unwrap();
    let ChatSubmission::Accepted { id } = result else {
        panic!("live store node must accept the share");
    };
    receive_events(&bob);
    let link = dyapp_identity::profile_link::profile_link(&alice_id).unwrap();
    let received = message(&bob, &alice_id, &id);
    assert_eq!(received.text.as_deref(), Some(link.as_str()));
    assert!(!received.forwarded);
    assert_eq!(texts(&bob, &alice_id), ["hello", "reply", link.as_str()]);
    assert_eq!(message(&alice, &bob_id, &id).id, received.id);
}
