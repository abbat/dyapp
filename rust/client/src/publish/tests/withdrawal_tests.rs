//! Mutations run at the server's readback boundary, before the waiting client can commit.
use super::*;
use crate::store;
use prost::Message as _;

fn at_tombstone_readback(
    action: impl FnOnce(&Profile) + Send + 'static,
) -> impl FnMut(&Request, &proto::ProfileResponse) + Send {
    let mut action = Some(action);
    move |request, response| {
        if !matches!(request, Request::Get(_)) {
            return;
        }
        let Some(record) = &response.record else {
            return;
        };
        let profile = Profile::decode(record.payload.as_slice()).unwrap();
        if !profile.deleted {
            return;
        }
        if let Some(action) = action.take() {
            action(&profile);
        }
    }
}

fn edit_with_stronger_proof(
    path: std::path::PathBuf,
    identity: Identity,
) -> impl FnOnce(&Profile) + Send {
    move |_| {
        let mut other = Store::open(&path, &identity).unwrap();
        other
            .update_profile_draft(|current| {
                current.name = "after request".into();
                current.interests = vec!["current".into()];
                Ok(())
            })
            .unwrap();
        let work = other.prepare_profile_work(&identity, 1, 1024).unwrap();
        assert!(other.commit_profile_work(&work).unwrap().solved);
    }
}

#[tokio::test]
async fn withdrawal_readback_cannot_replace_an_edit_committed_while_waiting() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    let request = Profile {
        name: "before request".into(),
        ..Default::default()
    };
    store.save_profile_draft(&request).unwrap();
    let (mut node, held) = profile_node_observed(
        Vec::new(),
        at_tombstone_readback(edit_with_stronger_proof(
            path,
            Identity::from_secret(&identity.secret()),
        )),
    )
    .await;
    let result = withdraw(&store, &mut node, &identity, &request)
        .await
        .unwrap();
    assert_eq!(result.name, "after request");
    assert_eq!(result.interests, vec!["current".to_owned()]);
    assert_eq!(store.profile_draft().unwrap(), Some(result.clone()));
    let remote = dyapp_profile::verify(held.lock().unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(result.version, remote.profile.version);
    assert!(remote.profile.deleted);
    assert!(result.pow_bits > remote.profile.pow_bits);
    assert!(store.profile_hidden().unwrap());
    let signed = held.lock().unwrap().clone().unwrap();
    store.search_profile_sent(&signed, 1000).unwrap();
    let shown = publish(&store, &mut node, &identity, &result)
        .await
        .unwrap();
    assert_eq!(shown.name, result.name);
    assert_eq!(
        (shown.pow_nonce, shown.pow_bits),
        (result.pow_nonce, result.pow_bits)
    );
    assert!(!store.profile_hidden().unwrap());
}

#[tokio::test]
async fn newer_local_publication_projection_supersedes_withdrawal_readback() {
    let identity = Identity::generate();
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    let request = Profile::default();
    store.save_profile_draft(&request).unwrap();
    let secret = identity.secret();
    let (mut node, held) = profile_node_observed(
        Vec::new(),
        at_tombstone_readback(move |tombstone| {
            let identity = Identity::from_secret(&secret);
            let other = Store::open(&path, &identity).unwrap();
            let newer = Profile {
                version: tombstone.version.checked_add(1).unwrap(),
                name: "newer publication".into(),
                ..Default::default()
            };
            other.save_profile_draft(&newer).unwrap();
            other.hold_search_profile(&newer.sign(&identity)).unwrap();
            other.set_profile_hidden(false).unwrap();
        }),
    )
    .await;
    assert!(matches!(
        withdraw(&store, &mut node, &identity, &request).await,
        Err(Failure::Store(store::Error::ProfileCompletionSuperseded))
    ));
    let saved = store.profile_draft().unwrap().unwrap();
    assert_eq!(saved.name, "newer publication");
    assert!(!store.profile_hidden().unwrap());
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(saved.sign(&identity))
    );
    assert!(
        dyapp_profile::verify(held.lock().unwrap().as_ref().unwrap())
            .unwrap()
            .profile
            .deleted
    );
}
