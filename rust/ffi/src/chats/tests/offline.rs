use super::favorites::Fixture;
use super::*;

fn establish(f: &Fixture) -> String {
    let peer = Identity::generate();
    let key = Identity::generate();
    let device = dyapp_messaging::mls::Device::new(&peer, key.public_key(), 1).unwrap();
    let profile = dyapp_profile::Profile {
        version: 1,
        devices: vec![device.certificate().clone()],
        ..Default::default()
    };
    let peer_id = peer.peer_id();
    let mut session = f.chats.session.lock().unwrap();
    let own = dyapp_profile::Profile {
        version: 1,
        devices: vec![session.certificate().unwrap()],
        ..Default::default()
    };
    session.store().save_profile_draft(&own).unwrap();
    let profiles = [
        dyapp_profile::verify(&own.sign(&f.key.identity)).unwrap(),
        dyapp_profile::verify(&profile.sign(&peer)).unwrap(),
    ];
    let packages = [(
        dyapp_client::mailbox(&key.public_key()).to_vec(),
        device.key_package(false).unwrap(),
    )];
    session
        .stage_text_to(&profiles, &packages, b"established", &f.chats.seal_key)
        .unwrap();
    let mut state = dyapp_client::store::Store::open(
        &std::path::Path::new(&f.dir).join("client.db"),
        &f.key.identity,
    )
    .unwrap();
    let like = dyapp_client::interactions::Interaction {
        kind: dyapp_client::interactions::Kind::Like,
        event_id: [1; 16],
        hlc: 1000,
    };
    state
        .apply_interaction(&f.key.peer_id(), &peer_id, &[1], like)
        .unwrap();
    state
        .apply_interaction(&peer_id, &f.key.peer_id(), &[2], like)
        .unwrap();
    peer_id
}

#[test]
fn established_native_text_queues_without_an_active_network() {
    let f = Fixture::new();
    let peer = establish(&f);
    let result = f
        .chats
        .submit_text(peer.clone(), "offline native".into())
        .unwrap();
    let crate::ChatSubmission::Queued { id, .. } = result else {
        panic!("offline submission must retain its committed ID");
    };
    let rows = f.chats.chat_history(peer).unwrap().messages;
    assert_eq!(rows.iter().filter(|row| row.id == id).count(), 1);
    assert_eq!(rows.last().unwrap().text.as_deref(), Some("offline native"));
}

fn snapshot(f: &Fixture) -> (Option<Vec<u8>>, Vec<Vec<u8>>) {
    let session = f.chats.session.lock().unwrap();
    let store = session.store();
    let writes = store
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .map(|write| write.as_bytes().to_vec())
        .collect();
    (store.sealed_device().unwrap(), writes)
}

#[test]
fn locally_unlisted_device_cannot_stage_offline_text_in_an_established_chat() {
    let f = Fixture::new();
    let peer = establish(&f);
    {
        let session = f.chats.session.lock().unwrap();
        let mut own = session.store().profile_draft().unwrap().unwrap();
        own.devices.clear();
        session.store().save_profile_draft(&own).unwrap();
    }
    let before = snapshot(&f);
    let history = f.chats.chat_history(peer.clone()).unwrap().messages;
    let result = f.chats.submit_text(peer.clone(), "must not commit".into());
    assert!(matches!(result, Err(crate::CoreError::Failed(reason)) if reason == "unpublished"));
    assert_eq!(snapshot(&f), before);
    assert_eq!(f.chats.chat_history(peer).unwrap().messages, history);
}

#[test]
fn first_contact_without_a_network_does_not_stage_or_advance_mls() {
    let f = Fixture::new();
    establish(&f);
    let before = snapshot(&f);
    let peer = Identity::generate().peer_id();
    let result = f.chats.submit_text(peer.clone(), "not committed".into());
    assert!(
        matches!(result, Err(crate::CoreError::Failed(reason)) if reason.contains("notConnected"))
    );
    assert_eq!(snapshot(&f), before);
    assert_eq!(f.chats.chat_history(peer).unwrap().messages.len(), 0);
}
