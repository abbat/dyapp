//! Client media calls: listed blobs and attachments are put, read back and released.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::attachment;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::interactions::Kind;
use dyapp_client::media_ownership::{Intent, Replaced};
use dyapp_client::publish;
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use dyapp_client::Error;
use std::slice::from_mut;

const SEAL: [u8; 32] = [6; 32];

/// Leaves only `hashes` listed under the current publication intent, as a node losing blobs would.
async fn keep_only(node: &mut dyapp_client::Node, owner: &Identity, hashes: Vec<Vec<u8>>) {
    let state = node.media_keep_state(owner).await.unwrap();
    let intent = Intent {
        profile_hash: state.intent_hash().unwrap(),
        pending: state.pending(),
    };
    let replaced = node
        .replace_media_keep(owner, &state, hashes, intent)
        .await
        .unwrap();
    assert!(matches!(replaced, Replaced::Applied { .. }));
}

async fn media_node() -> Multiaddr {
    media_node_at(10).await
}

async fn media_node_at(media_requests_per_second: u32) -> Multiaddr {
    start_config(|config| {
        config.roles = vec![Role::Store, Role::Media];
        config.limits.requests_per_second = 1000;
        config.limits.media_requests_per_second = media_requests_per_second;
        config.limits.min_free_mb = 0;
        config.limits.require_write_tokens = false;
    })
    .await
}

#[tokio::test]
async fn the_daily_keep_puts_back_a_lost_photo_of_the_live_profile() {
    let address = media_node_at(1000).await;
    let connect = || {
        dyapp_client::Node::connect(
            address.clone(),
            std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
        )
    };
    let (mut node, mut reader) = (connect().await.unwrap(), connect().await.unwrap());
    let owner = Identity::generate();
    let mut store = Store::open(&store_path("photo-keep"), &owner).unwrap();
    let (full, thumbnail) = (vec![0xFF, 0xD8, 0xFF, 1], vec![0xFF, 0xD8, 0xFF, 2]);
    let draft = store.add_profile_photo(&full, &thumbnail).unwrap();
    let published = publish::publish(&store, &mut node, &owner, &draft)
        .await
        .unwrap();
    let photo = published.photos[0].clone();
    let hash: [u8; 32] = photo.full[0].clone().try_into().unwrap();
    let small = vec![photo.thumbnail.clone()];
    let now = 1_000_000;

    // The node dropped the full image from the list of the live profile.
    keep_only(&mut node, &owner, small.clone()).await;
    assert!(reader.get_media(&hash).await.is_err());
    publish::refresh_photos(&store, &mut node, &owner, now)
        .await
        .unwrap();
    assert_eq!(reader.get_media(&hash).await.unwrap(), full);

    // Within a day nothing is sent, so the next loss stays until the day has passed.
    keep_only(&mut node, &owner, small).await;
    publish::refresh_photos(&store, &mut node, &owner, now + 86_399)
        .await
        .unwrap();
    assert!(reader.get_media(&hash).await.is_err());

    // An unpublished removal changes nothing for readers of the live profile.
    assert_eq!(
        store.remove_profile_photo(0).unwrap().photos,
        [] as [dyapp_profile::Photo; 0]
    );
    publish::refresh_photos(&store, &mut node, &owner, now + 86_400)
        .await
        .unwrap();
    assert_eq!(reader.get_media(&hash).await.unwrap(), full);
}

#[tokio::test]
async fn six_photos_fit_the_default_rate_of_a_node() {
    let address = media_node().await;
    let protocols = std::sync::Arc::new(dyapp_p2p_net::Protocols::public());
    let mut node = dyapp_client::Node::connect(address, protocols)
        .await
        .unwrap();
    let owner = Identity::generate();
    let mut store = Store::open(&store_path("six-photos"), &owner).unwrap();
    for seed in 1..=6 {
        store
            .add_profile_photo(&[0xFF, 0xD8, 0xFF, seed], &[0xFF, 0xD8, 0xFF, 0x10 + seed])
            .unwrap();
    }
    let draft = store.profile_draft().unwrap().unwrap();
    publish::publish(&store, &mut node, &owner, &draft)
        .await
        .unwrap();
}

#[tokio::test]
async fn profile_photos_reach_readers_and_leave_with_the_profile() {
    let address = media_node().await;
    let connect = || {
        dyapp_client::Node::connect(
            address.clone(),
            std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
        )
    };
    let (mut node, mut reader) = (connect().await.unwrap(), connect().await.unwrap());
    let owner = Identity::generate();
    let mut store = Store::open(&store_path("profile-photos"), &owner).unwrap();
    let (full, thumbnail) = (vec![0xFF, 0xD8, 0xFF, 1], vec![0xFF, 0xD8, 0xFF, 2]);

    let draft = store.add_profile_photo(&full, &thumbnail).unwrap();
    let published = publish::publish(&store, &mut node, &owner, &draft)
        .await
        .unwrap();
    let seen = reader.get_profile(&owner.peer_id()).await.unwrap().profile;
    assert_eq!(seen.photos, published.photos);
    let photo = &seen.photos[0];
    let hash: [u8; 32] = photo.full[0].clone().try_into().unwrap();
    let small: [u8; 32] = photo.thumbnail.clone().try_into().unwrap();
    assert_eq!(reader.get_media(&hash).await.unwrap(), full);
    assert_eq!(reader.get_media(&small).await.unwrap(), thumbnail);

    // A republish finds the blobs held; a failed publish left another list behind.
    let published = publish::publish(&store, &mut node, &owner, &published)
        .await
        .unwrap();
    keep_only(&mut node, &owner, vec![published.photos[0].thumbnail.clone()]).await;

    let draft = store.remove_profile_photo(0).unwrap();
    assert!(store.has_profile_photos().unwrap());
    publish::publish(&store, &mut node, &owner, &draft)
        .await
        .unwrap();
    assert!(!store.has_profile_photos().unwrap());
    assert!(matches!(
        reader.get_media(&hash).await,
        Err(Error::Status(Status::NotFound))
    ));

    // Withdrawing takes the photos off the node with the profile.
    let draft = store.add_profile_photo(&full, &thumbnail).unwrap();
    let published = publish::publish(&store, &mut node, &owner, &draft)
        .await
        .unwrap();
    assert_eq!(reader.get_media(&hash).await.unwrap(), full);
    publish::withdraw(&store, &mut node, &owner, &published)
        .await
        .unwrap();
    assert!(matches!(
        reader.get_media(&hash).await,
        Err(Error::Status(Status::NotFound))
    ));
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
    for owner in [alice.identity.peer_id(), bob_owner.clone()] {
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
    // A match opens the chat: an attachment is no message request.
    alice_session
        .interact(from_mut(&mut alice.node), &group_id, Kind::Like, &SEAL)
        .await
        .unwrap();
    bob_session
        .interact(from_mut(&mut bob.node), &group_id, Kind::Like, &SEAL)
        .await
        .unwrap();
    bob_session
        .receive(from_mut(&mut bob.node), &bob.device_key, &profiles, &SEAL)
        .await
        .unwrap();

    alice_session
        .receive(
            from_mut(&mut alice.node),
            &alice.device_key,
            &profiles,
            &SEAL,
        )
        .await
        .unwrap();
    let photo = b"a photo only bob may see".repeat(4096);
    let (mut sent, sealed) = attachment::seal(&photo).unwrap();
    sent.preview = b"preview".to_vec();
    sent.mime = "image/jpeg".into();
    let sent_id = alice_session
        .send_attachment_to(
            from_mut(&mut alice.node),
            &alice.identity,
            &bob_owner,
            &sent,
            &sealed,
            &SEAL,
        )
        .await
        .unwrap();
    let own = alice_session
        .store()
        .current_chat_attachments(&bob_owner)
        .unwrap();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].key.id, sent_id);
    assert_eq!(
        own[0].delivery,
        Some(dyapp_client::store::messages::DeliveryState::NodeAccepted)
    );
    let sealed = bob.node.get_media(&sent.blob_sha256).await.unwrap();
    assert!(!sealed.windows(16).any(|window| photo.starts_with(window)));
    assert!(!sealed.windows(32).any(|window| window == sent.key));

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
    assert_eq!(received.id, sent_id);
    assert!(received.hlc > 0);
    assert_eq!(received.attachment, sent);
    assert_eq!(
        attachment::fetch(from_mut(&mut bob.node), &received.attachment)
            .await
            .unwrap(),
        photo
    );
    bob.node
        .release_media(&received.attachment.release_secret)
        .await
        .unwrap();
}
