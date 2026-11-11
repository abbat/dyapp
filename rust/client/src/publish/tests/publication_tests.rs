//! Readback observers establish transitions before the waiting client can project local state.
use super::*;
use prost::Message as _;

fn at_readback(
    action: impl FnOnce(&Profile) + Send + 'static,
) -> impl FnMut(&Request, &proto::ProfileResponse) + Send {
    let (mut accepted, mut action) = (None, Some(action));
    move |request, response| match request {
        Request::Publish(record) if response.status == i32::from(Status::Ok) => {
            accepted = Some(record.clone());
        }
        Request::Get(_) => {
            let Some(record) = &response.record else {
                return;
            };
            if Some(record) != accepted.as_ref() {
                return;
            }
            if let Some(action) = action.take() {
                action(&Profile::decode(record.payload.as_slice()).unwrap());
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn prepared_baseline_covers_edits_during_the_earlier_connection_await() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    store
        .save_profile_draft(&Profile {
            name: "prepared".into(),
            ..Default::default()
        })
        .unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let (mut node, _) = profile_node(Vec::new()).await;
    let mut other = Store::open(&path, &identity).unwrap();
    other
        .update_profile_draft(|draft| {
            draft.name = "during connect".into();
            Ok(())
        })
        .unwrap();
    let outcome = crate::publish::publish_prepared(&store, &mut node, &identity, &attempt)
        .await
        .unwrap();
    assert_eq!(outcome.published.name, "prepared");
    assert_eq!(outcome.draft.name, "during connect");
    assert!(outcome.pending);
}

fn edit_at_readback(path: std::path::PathBuf, identity: Identity) -> impl FnOnce(&Profile) + Send {
    move |_| {
        let mut other = Store::open(&path, &identity).unwrap();
        other
            .update_profile_draft(|draft| {
                draft.name = "during readback".into();
                draft.interests = vec!["pending".into()];
                draft.devices.clear();
                draft.device_owners.clear();
                Ok(())
            })
            .unwrap();
        other
            .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
            .unwrap();
        let work = other.prepare_profile_work(&identity, 1, 1024).unwrap();
        assert!(other.commit_profile_work(&work).unwrap().solved);
    }
}

#[tokio::test]
async fn readback_keeps_pending_fields_photos_proof_and_device_removal() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    let key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    let base = Profile {
        devices: vec![device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&key, &identity.public_key())],
        ..Default::default()
    };
    store.save_profile_draft(&base).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let (mut node, held) = profile_node_observed(
        Vec::new(),
        at_readback(edit_at_readback(
            path,
            Identity::from_secret(&identity.secret()),
        )),
    )
    .await;
    let outcome = crate::publish::publish_prepared(&store, &mut node, &identity, &attempt)
        .await
        .unwrap();
    assert_eq!(outcome.published, Profile { version: 1, ..base });
    assert_eq!(outcome.draft.name, "during readback");
    assert_eq!(outcome.draft.interests, vec!["pending".to_owned()]);
    assert_eq!(outcome.draft.devices, Vec::new());
    assert_eq!(outcome.draft.device_owners, Vec::new());
    assert_eq!(outcome.draft.photos.len(), 1);
    assert_eq!(outcome.draft.pow_bits, 1);
    assert!(outcome.pending);
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        held.lock().unwrap().clone()
    );
}

#[tokio::test]
async fn prepared_hidden_relist_distinguishes_published_and_search_versions() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_profile_hidden(true).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let (mut node, held) = profile_node(Vec::new()).await;
    let outcome = crate::publish::relist_prepared(&store, &mut node, &identity, &attempt)
        .await
        .unwrap();
    let remote = dyapp_profile::verify(held.lock().unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(outcome.published, remote.profile);
    assert_eq!(
        outcome.draft.version,
        outcome.published.version.checked_add(1).unwrap()
    );
    assert!(outcome.hidden);
    assert!(!outcome.pending);
    let search = Profile::decode(
        store
            .due_search_profile(0)
            .unwrap()
            .unwrap()
            .payload
            .as_slice(),
    )
    .unwrap();
    assert!(search.deleted);
    assert_eq!(search.version, outcome.draft.version);
}

#[tokio::test]
async fn hide_at_readback_cannot_be_undone_by_the_older_publish_completion() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let copy = Identity::from_secret(&identity.secret());
    let (mut node, _) = profile_node_observed(
        Vec::new(),
        at_readback(move |_| {
            let other = Store::open(&path, &copy).unwrap();
            other
                .hide_profile_draft(&copy, &Profile::default())
                .unwrap();
        }),
    )
    .await;
    assert!(matches!(
        crate::publish::publish_prepared(&store, &mut node, &identity, &attempt).await,
        Err(Failure::Store(
            crate::store::Error::ProfileCompletionSuperseded
        ))
    ));
    assert!(store.profile_hidden().unwrap());
    assert!(
        Profile::decode(
            store
                .due_search_profile(0)
                .unwrap()
                .unwrap()
                .payload
                .as_slice()
        )
        .unwrap()
        .deleted
    );
}
