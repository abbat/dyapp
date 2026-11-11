use super::*;

#[tokio::test]
async fn withdrawal_uses_independent_conditional_media_revisions() {
    let identity = Identity::generate();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let request = Profile {
        version: 7,
        ..Default::default()
    };
    store.save_profile_draft(&request).unwrap();
    store
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
        .unwrap();
    let keeps = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&keeps);
    let (mut node, held) = node::observed(
        Vec::new(),
        |_, _| {},
        move |keep| {
            observed.lock().unwrap().push(keep.clone());
        },
    )
    .await;
    let result = withdraw(&store, &mut node, &identity, &request)
        .await
        .unwrap();
    let remote = dyapp_profile::verify(held.lock().unwrap().as_ref().unwrap()).unwrap();
    let keeps = keeps.lock().unwrap();
    assert_eq!(keeps.len(), 2);
    assert_eq!(
        keeps.iter().map(|keep| keep.version).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(keeps.iter().all(|keep| keep.hashes.is_empty()));
    assert_eq!(result.version, remote.profile.version);
    assert!(store.profile_hidden().unwrap());
    assert_eq!(result.photos.len(), 1);
    assert!(store
        .profile_photo(&result.photos[0].thumbnail)
        .unwrap()
        .is_some());
}
