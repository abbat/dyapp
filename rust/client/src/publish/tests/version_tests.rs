//! Exercise real publication/adoption/media paths at the representable version boundary.
use super::*;

#[tokio::test]
async fn publication_refuses_max_but_accepts_the_last_representable_successor() {
    let identity = Identity::generate();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let exhausted = Profile {
        version: u64::MAX,
        name: "keep".into(),
        ..Default::default()
    };
    store.save_profile_draft(&exhausted).unwrap();
    let (mut node, held) = profile_node(Vec::new()).await;
    assert!(matches!(
        publish(&store, &mut node, &identity, &exhausted).await,
        Err(Failure::Store(crate::store::Error::ProfileVersionExhausted))
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(exhausted));
    assert!(held.lock().unwrap().is_none());
    let other = Store::open(&temp_db_path(), &identity).unwrap();
    let last = Profile {
        version: u64::MAX - 1,
        ..Default::default()
    };
    let published = publish(&other, &mut node, &identity, &last).await.unwrap();
    assert_eq!(published.version, u64::MAX);
    assert_eq!(other.profile_draft().unwrap(), Some(published.clone()));
    assert_eq!(
        other.due_search_profile(u64::MAX).unwrap(),
        Some(published.sign(&identity))
    );
}

fn state(identity: &Identity, hidden: bool) -> Vec<u8> {
    let source = Store::open(&temp_db_path(), identity).unwrap();
    source.set_profile_hidden(hidden).unwrap();
    source.link_state().unwrap()
}

#[tokio::test]
async fn hidden_adoption_refuses_exhaustion_without_replacing_the_existing_draft() {
    let identity = Identity::generate();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = Profile {
        version: 4,
        name: "local".into(),
        ..Default::default()
    };
    store.save_profile_draft(&before).unwrap();
    let (mut node, held) = profile_node(Vec::new()).await;
    *held.lock().unwrap() = Some(
        Profile {
            version: u64::MAX,
            ..Default::default()
        }
        .sign(&identity),
    );
    assert!(matches!(
        crate::publish::adopt(
            &mut store,
            &mut node,
            &identity,
            &state(&identity, true),
            100
        )
        .await,
        Err(Failure::Store(crate::store::Error::ProfileVersionExhausted))
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(before));
    assert_eq!(store.own_profile_seen().unwrap(), 0);
    // Link-state import precedes network work and remains applied, as documented.
    assert!(store.profile_hidden().unwrap());
}

#[tokio::test]
async fn visible_max_and_hidden_last_successor_adoption_remain_supported() {
    for (hidden, version) in [(false, u64::MAX), (true, u64::MAX - 1)] {
        let identity = Identity::generate();
        let mut store = Store::open(&temp_db_path(), &identity).unwrap();
        let (mut node, held) = profile_node(Vec::new()).await;
        *held.lock().unwrap() = Some(
            Profile {
                version,
                ..Default::default()
            }
            .sign(&identity),
        );
        let adopted = crate::publish::adopt(
            &mut store,
            &mut node,
            &identity,
            &state(&identity, hidden),
            100,
        )
        .await
        .unwrap();
        assert_eq!(adopted.version, u64::MAX);
        assert_eq!(store.profile_draft().unwrap(), Some(adopted));
        assert_eq!(store.own_profile_seen().unwrap(), version);
        assert_eq!(store.profile_hidden().unwrap(), hidden);
    }
}

async fn refused_media(initial: u64, remote: u64) -> (Result<(), Failure>, Vec<u64>) {
    let identity = Identity::generate();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let versions = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&versions);
    let (mut node, held) = node::observed(
        Vec::new(),
        |_, _| {},
        move |keep| {
            observed.lock().unwrap().push(keep.version);
        },
    )
    .await;
    *held.lock().unwrap() = Some(
        Profile {
            version: remote,
            ..Default::default()
        }
        .sign(&identity),
    );
    let result = crate::publish::upload_photos(&store, &mut node, &identity, initial, &[]).await;
    let sent = versions.lock().unwrap().clone();
    (result, sent)
}

#[tokio::test]
async fn stale_media_retry_never_wraps_local_or_remote_version_floors() {
    for (initial, remote, expected) in [
        (u64::MAX, 1, vec![u64::MAX]),
        (u64::MAX - 1, 1, vec![u64::MAX - 1, u64::MAX]),
        (10, u64::MAX, vec![10]),
    ] {
        let (result, versions) = refused_media(initial, remote).await;
        assert!(matches!(
            result,
            Err(Failure::Store(crate::store::Error::ProfileVersionExhausted))
        ));
        assert_eq!(versions, expected);
    }
}

#[tokio::test]
async fn non_exhausted_media_retry_keeps_the_existing_refusal_budget_and_status() {
    let (result, versions) = refused_media(1, 0).await;
    assert!(matches!(
        result,
        Err(Failure::Node(Error::Status(Status::Stale)))
    ));
    assert_eq!(
        versions,
        (1..=u64::from(crate::publish::KEEP_REFUSALS) + 1).collect::<Vec<_>>()
    );
}
