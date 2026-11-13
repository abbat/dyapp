//! Remote response observers commit edits before adoption resumes, without timing assumptions.
use super::*;

#[tokio::test]
async fn adoption_refuses_another_owner_before_applying_the_private_prefix() {
    let identity = Identity::from_secret(&[7; 32]);
    let stranger = Identity::from_secret(&[8; 32]);
    let source = Store::open(&temp_db_path(), &stranger).unwrap();
    source.set_blocked("carol", true).unwrap();
    source.set_profile_hidden(true).unwrap();
    let state = source.link_state().unwrap();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    let before = store.link_state().unwrap();
    let (mut node, _) = profile_node(Vec::new()).await;
    assert!(matches!(
        crate::publish::adopt(&mut store, &mut node, &stranger, &state, 100).await,
        Err(Failure::Store(crate::store::Error::WrongIdentity))
    ));
    assert_eq!(store.link_state().unwrap(), before);
    assert_eq!(store.link_pending().unwrap(), Some(state));
}

#[tokio::test]
async fn review_refuses_wrong_owner_deleted_remote_and_missing_pending_link() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let local = Profile {
        name: "keep".into(),
        ..Default::default()
    };
    store.save_profile_draft(&local).unwrap();
    store.set_link_pending(b"pending state").unwrap();
    let (mut node, held) = profile_node(Vec::new()).await;
    *held.lock().unwrap() = Some(Profile::tombstone(3, 0, 0).sign(&identity));
    let stranger = Identity::from_secret(&[8; 32]);
    assert!(matches!(
        crate::publish::review_adoption(&store, &mut node, &stranger).await,
        Err(Failure::Store(crate::store::Error::WrongIdentity))
    ));
    assert!(matches!(
        crate::publish::review_adoption(&store, &mut node, &identity).await,
        Err(Failure::AdoptionReviewRequired)
    ));
    let fresh = Store::open(&temp_db_path(), &identity).unwrap();
    assert!(matches!(
        crate::publish::review_adoption(&fresh, &mut node, &identity).await,
        Err(Failure::Node(Error::Input(
            "no pending linked-device recovery"
        )))
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(local));
    assert_eq!(
        store.link_pending().unwrap(),
        Some(b"pending state".to_vec())
    );
    assert_eq!(fresh.profile_draft().unwrap(), None);
}

#[tokio::test]
async fn review_freezes_local_state_before_the_remote_reply_without_replacing_newer_edits() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    let local = Profile {
        version: 1,
        name: "before review".into(),
        ..Default::default()
    };
    store.save_profile_draft(&local).unwrap();
    store.set_link_pending(b"pending state").unwrap();
    let copy = Identity::from_secret(&identity.secret());
    let mut pending = Some((path, copy));
    let (mut node, held) = profile_node_observed(Vec::new(), move |request, _| {
        if !matches!(request, Request::Get(_)) {
            return;
        }
        if let Some((path, owner)) = pending.take() {
            Store::open(&path, &owner)
                .unwrap()
                .update_profile_draft(|draft| {
                    draft.name = "after review started".into();
                    Ok(())
                })
                .unwrap();
        }
    })
    .await;
    let remote = Profile {
        version: 3,
        name: "remote".into(),
        ..Default::default()
    };
    *held.lock().unwrap() = Some(remote.sign(&identity));
    let review = crate::publish::review_adoption(&store, &mut node, &identity)
        .await
        .unwrap();
    assert_eq!(review.local_profile(), &local);
    assert_eq!(review.remote_profile(), &remote);
    assert!(!review.is_current(&store, &identity).unwrap());
    assert_eq!(
        store.profile_draft().unwrap().unwrap().name,
        "after review started"
    );
    assert_eq!(
        store.link_pending().unwrap(),
        Some(b"pending state".to_vec())
    );
    assert_eq!(store.own_profile_seen().unwrap(), 0);
    assert!(!store.profile_hidden().unwrap());
    assert_eq!(held.lock().unwrap().as_ref(), Some(&remote.sign(&identity)));
}

#[tokio::test]
async fn adoption_keeps_edits_made_during_its_initial_remote_read() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let mut store = Store::open(&path, &identity).unwrap();
    let baseline = Profile {
        version: 1,
        name: "remote".into(),
        profession: "remote job".into(),
        ..Default::default()
    };
    store.save_profile_draft(&baseline).unwrap();
    let link_state = Store::open(&temp_db_path(), &identity)
        .unwrap()
        .link_state()
        .unwrap();
    let remote = Profile {
        version: 3,
        ..baseline
    };
    let copy = Identity::from_secret(&identity.secret());
    let mut pending = Some((path, copy));
    let (mut node, held) = profile_node_observed(Vec::new(), move |request, _| {
        if !matches!(request, Request::Get(_)) {
            return;
        }
        if let Some((path, owner)) = pending.take() {
            let mut other = Store::open(&path, &owner).unwrap();
            other
                .update_profile_draft(|draft| {
                    draft.name = "during adoption".into();
                    Ok(())
                })
                .unwrap();
            other.set_profile_hidden(true).unwrap();
        }
    })
    .await;
    *held.lock().unwrap() = Some(remote.sign(&identity));
    let outcome = crate::publish::adopt(&mut store, &mut node, &identity, &link_state, 100)
        .await
        .unwrap();
    assert_eq!(outcome.published, remote);
    assert_eq!(outcome.draft.name, "during adoption");
    assert_eq!(outcome.draft.profession, "remote job");
    assert_eq!(outcome.draft.version, 4);
    assert!(outcome.hidden && outcome.pending);
    assert!(store.profile_hidden().unwrap());
    assert_eq!(store.own_profile_seen().unwrap(), 3);
    assert_eq!(held.lock().unwrap().as_ref(), Some(&remote.sign(&identity)));
}
