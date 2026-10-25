//! Client media calls: listed blobs and attachments are put, read back and released.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::attachment;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use dyapp_client::Error;
use std::slice::from_mut;

const SEAL: [u8; 32] = [6; 32];

async fn media_node() -> Multiaddr {
    start_config(|config| {
        config.roles = vec![Role::Store, Role::Media];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
        config.limits.require_write_tokens = false;
    })
    .await
}

#[tokio::test]
async fn client_keeps_attaches_reads_and_releases_media() {
    let address = media_node().await;
    let mut node = dyapp_client::Node::connect(
        address,
        std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
    )
    .await
    .unwrap();
    let owner = Identity::generate();

    // A kept blob is accepted, read back whole and checked against its hash.
    let photo = b"profile photo".to_vec();
    let hash = dyapp_identity::sha256(&photo);
    node.keep_media(&owner, 1, vec![hash.to_vec()])
        .await
        .unwrap();
    node.put_media(&owner.peer_id(), photo.clone())
        .await
        .unwrap();
    assert_eq!(node.get_media(&hash).await.unwrap(), photo);
    assert!(matches!(
        node.put_media(&owner.peer_id(), b"unlisted".to_vec()).await,
        Err(Error::Status(_))
    ));

    // An attachment blob is put after its attach and dropped once released.
    let (sealed, blob) = attachment::seal(b"chat photo").unwrap();
    assert_eq!(
        node.attach_media(&owner, &sealed).await.unwrap(),
        [sealed.blob_sha256.to_vec()]
    );
    node.put_media(&owner.peer_id(), blob.clone())
        .await
        .unwrap();
    let fetched = node.get_media(&sealed.blob_sha256).await.unwrap();
    assert_eq!(attachment::open(&sealed, &fetched).unwrap(), b"chat photo");
    node.release_media(&sealed.release_secret).await.unwrap();
    assert!(matches!(
        node.get_media(&sealed.blob_sha256).await,
        Err(Error::Status(Status::NotFound))
    ));
    assert!(matches!(
        node.release_media(&sealed.release_secret).await,
        Err(Error::Status(Status::NotFound))
    ));
}

#[tokio::test]
async fn an_attachment_reaches_the_member_while_the_node_holds_only_ciphertext() {
    let address = media_node().await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice.identity.peer_id(), bob_owner] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let group_id = group.id().to_vec();
    let mut store = Store::open(&store_path("media-alice"), &alice.identity).unwrap();
    let sealed = alice.device.seal(&SEAL).unwrap();
    store
        .stage_group(&group, &profiles[1], &sealed, &[])
        .unwrap();
    let mut alice_session = Session::open(store, &SEAL).unwrap();
    let mut store = Store::open(&store_path("media-bob"), &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut bob_session = Session::open(store, &SEAL).unwrap();
    bob_session
        .receive(from_mut(&mut bob.node), &bob.device_key, &profiles, &SEAL)
        .await
        .unwrap();

    let photo = b"a photo only bob may see".repeat(4096);
    let sent = alice_session
        .send_attachment(
            from_mut(&mut alice.node),
            &alice.identity,
            &group_id,
            &photo,
            &SEAL,
        )
        .await
        .unwrap();
    assert_eq!(sent.size, photo.len() as u64);
    let blob = bob.node.get_media(&sent.blob_sha256).await.unwrap();
    assert!(!blob.windows(16).any(|window| photo.starts_with(window)));
    assert!(!blob.windows(32).any(|window| window == sent.key));

    let events = bob_session
        .receive(from_mut(&mut bob.node), &bob.device_key, &profiles, &SEAL)
        .await
        .unwrap();
    let [Event::Message {
        body: Body::Attachment(received),
        ..
    }] = events.as_slice()
    else {
        panic!("expected one attachment, got {events:?}");
    };
    assert_eq!(*received, sent);
    assert_eq!(
        attachment::fetch(from_mut(&mut bob.node), received)
            .await
            .unwrap(),
        photo
    );
    bob.node
        .release_media(&received.release_secret)
        .await
        .unwrap();
}
