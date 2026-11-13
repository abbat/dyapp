//! The node observer commits independent local edits before returning the foreign profile.
use super::*;

#[tokio::test]
async fn foreign_import_keeps_only_post_prepare_edits_and_the_exact_remote_version() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    let baseline = Profile {
        version: 1,
        name: "before".into(),
        profession: "old job".into(),
        ..Default::default()
    };
    store.save_profile_draft(&baseline).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let foreign = Profile {
        version: 3,
        name: "foreign".into(),
        profession: "foreign job".into(),
        ..baseline
    };
    let copy = Identity::from_secret(&identity.secret());
    let mut pending = Some((path, copy));
    let (mut node, held) = profile_node_observed(Vec::new(), move |request, _| {
        if !matches!(request, Request::Get(_)) {
            return;
        }
        if let Some((path, identity)) = pending.take() {
            Store::open(&path, &identity)
                .unwrap()
                .update_profile_draft(|draft| {
                    draft.name = "during import".into();
                    Ok(())
                })
                .unwrap();
        }
    })
    .await;
    *held.lock().unwrap() = Some(foreign.sign(&identity));
    let result = crate::publish::publish_prepared(&store, &mut node, &identity, &attempt).await;
    assert!(matches!(result, Err(Failure::Missed)));
    let current = store.profile_draft().unwrap().unwrap();
    assert_eq!(current.name, "during import");
    assert_eq!(current.profession, "foreign job");
    assert_eq!(current.version, foreign.version);
    assert_eq!(
        held.lock().unwrap().as_ref(),
        Some(&foreign.sign(&identity))
    );
}
