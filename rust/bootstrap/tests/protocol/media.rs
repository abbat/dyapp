//! Client media calls: listed blobs and attachments are put, read back and released.
use super::*;
use dyapp_client::attachment;
use dyapp_client::Error;

#[tokio::test]
async fn client_keeps_attaches_reads_and_releases_media() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store, Role::Media];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
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
    node.keep_media(&owner, 1, vec![hash.to_vec()]).await.unwrap();
    node.put_media(&owner.peer_id(), photo.clone()).await.unwrap();
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
    node.put_media(&owner.peer_id(), blob.clone()).await.unwrap();
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
