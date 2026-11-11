use super::*;

#[tokio::test]
async fn withdrawal_never_advances_past_a_refused_media_release_version() {
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
    assert_eq!(keeps.len(), 1);
    assert_eq!(keeps[0].version, remote.profile.version);
    assert_eq!(keeps[0].hashes, Vec::<Vec<u8>>::new());
    assert_eq!(result.version, remote.profile.version);
    assert!(store.profile_hidden().unwrap());
    assert_eq!(result.photos.len(), 1);
    assert!(store
        .profile_photo(&result.photos[0].thumbnail)
        .unwrap()
        .is_some());
}
