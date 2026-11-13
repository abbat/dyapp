//! Remote response observers commit edits before adoption resumes, without timing assumptions.
use super::*;

#[tokio::test]
async fn adoption_keeps_edits_made_during_its_initial_remote_read() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let mut store = Store::open(&path, &identity).unwrap();
    let baseline = Profile {
        version: 1,
        name: "before".into(),
        profession: "old job".into(),
        ..Default::default()
    };
    store.save_profile_draft(&baseline).unwrap();
    let link_state = Store::open(&temp_db_path(), &identity)
        .unwrap()
        .link_state()
        .unwrap();
    let remote = Profile {
        version: 3,
        name: "remote".into(),
        profession: "remote job".into(),
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
