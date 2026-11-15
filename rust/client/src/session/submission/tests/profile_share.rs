use super::*;
use crate::forwarding::tests::open_chat;

fn unstaged(f: &Fixture, sealed: Option<&[u8]>, before: &[Vec<u8>]) {
    assert_eq!(
        f.session.store().sealed_device().unwrap().as_deref(),
        sealed
    );
    assert_eq!(writes(&f.session), before);
    assert!(f
        .session
        .store()
        .current_chat_messages(&f.peer)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn invalid_links_and_same_own_or_missing_targets_never_stage_a_share() {
    let mut f = fixture();
    open_chat(&mut f.session.store, &f.peer);
    let shared = f.owner.peer_id();
    let sealed = f.session.store().sealed_device().unwrap();
    let before = writes(&f.session);
    for (target, link) in [
        (f.peer.clone(), String::new()),
        (f.peer.clone(), "AB".repeat(32)),
        (f.peer.clone(), f.peer.clone()),
        (shared.clone(), f.peer.clone()),
        ("ef".repeat(32), shared.clone()),
        ("invalid".into(), shared.clone()),
    ] {
        assert!(matches!(
            f.session
                .submit_established_profile_share(&mut [], &target, &link, &f.seal)
                .await,
            Err(Error::InvalidProfileShare)
        ));
        unstaged(&f, sealed.as_deref(), &before);
    }
}

#[tokio::test]
async fn a_previously_open_target_is_rechecked_by_both_submission_paths() {
    for blocked in [false, true] {
        let mut f = fixture();
        open_chat(&mut f.session.store, &f.peer);
        let shared = f.owner.peer_id();
        assert!(f.session.prepare_profile_share(&f.peer, &shared).is_ok());
        if blocked {
            f.session.store.set_blocked(&f.peer, true).unwrap();
        } else {
            f.session
                .store
                .interact(&f.peer, crate::interactions::Kind::Unlike, &[3], 2_000)
                .unwrap();
        }
        let sealed = f.session.store().sealed_device().unwrap();
        let before = writes(&f.session);
        let online = f
            .session
            .submit_profile_share_to(&mut [], &f.profiles, &shared, &f.seal)
            .await;
        let offline = f
            .session
            .submit_established_profile_share(&mut [], &f.peer, &shared, &f.seal)
            .await;
        for result in [online, offline] {
            if blocked {
                assert!(matches!(
                    result,
                    Err(Error::Store(crate::store::Error::BlockedPeer))
                ));
            } else {
                assert!(matches!(result, Err(Error::InvalidProfileShare)));
            }
        }
        unstaged(&f, sealed.as_deref(), &before);
    }
}

#[tokio::test]
async fn an_unopened_pair_cannot_receive_a_share_as_an_introduction() {
    let mut f = fixture();
    let shared = f.owner.peer_id();
    let sealed = f.session.store().sealed_device().unwrap();
    let before = writes(&f.session);
    assert!(matches!(
        f.session
            .submit_profile_share_to(&mut [], &f.profiles, &shared, &f.seal)
            .await,
        Err(Error::InvalidProfileShare)
    ));
    assert!(matches!(
        f.session
            .submit_established_profile_share(&mut [], &f.peer, &shared, &f.seal)
            .await,
        Err(Error::InvalidProfileShare)
    ));
    unstaged(&f, sealed.as_deref(), &before);
}

#[tokio::test]
async fn queued_share_keeps_one_ordinary_link_and_exact_writes_after_reopen_and_retry() {
    for online in [false, true] {
        let mut f = fixture();
        open_chat(&mut f.session.store, &f.peer);
        let shared = f.owner.peer_id();
        let text = dyapp_identity::profile_link::profile_link(&shared).unwrap();
        let result = if online {
            f.session
                .submit_profile_share_to(&mut [], &f.profiles, &shared, &f.seal)
                .await
        } else {
            f.session
                .submit_established_profile_share(&mut [], &f.peer, &shared, &f.seal)
                .await
        }
        .unwrap();
        let id = result.id();
        assert!(matches!(result, Submission::Queued { .. }));
        let before = writes(&f.session);
        assert_ne!(before, [] as [Vec<u8>; 0]);
        drop(f.session);
        let mut reopened = Session::open(Store::open(&f.path, &f.owner).unwrap(), &f.seal).unwrap();
        assert_eq!(writes(&reopened), before);
        reopened.retry_due(&mut [], 0, &f.seal).await.unwrap();
        assert_eq!(writes(&reopened), before);
        let rows = reopened.store().current_chat_messages(&f.peer).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key.id, id);
        assert_eq!(rows[0].current.text.as_deref(), Some(text.as_bytes()));
        assert!(!rows[0].forwarded);
    }
}
