use super::*;

fn apply(
    f: &mut Fixture,
    kind: Kind,
    generation: Option<[u8; 16]>,
) -> Result<Option<Stage>, Error> {
    prepare(
        &mut f.store,
        &SEAL,
        &f.peer,
        &f.key,
        Action { kind, generation },
    )
}

#[test]
fn initial_action_rejects_generation_and_unpublished_but_duplicate_needs_no_mls() {
    let mut f = Fixture::new(true, false);
    assert!(matches!(
        apply(&mut f, Kind::Request, Some([3; 16])),
        Err(Error::RequestChanged)
    ));
    assert!(matches!(
        apply(&mut f, Kind::Request, None),
        Err(Error::Unpublished)
    ));
    let pending = request(&mut f);
    let sealed = f.store.sealed_device().unwrap();
    assert!(apply(&mut f, Kind::Request, None).unwrap().is_none());
    assert_eq!(f.store.sealed_device().unwrap(), sealed);
    assert!(f.store.pending_sticker_pack(&f.peer, f.pack.id()).unwrap() == Some(pending));
    assert!(f.store.due_outbound(i64::MAX).unwrap().is_empty());
}

fn assert_stale(f: &mut Fixture, before: &Status) {
    for kind in [Kind::Retry, Kind::Fresh, Kind::StopWaiting] {
        for generation in [None, Some([0; 16])] {
            assert!(matches!(
                apply(f, kind, generation),
                Err(Error::RequestChanged)
            ));
            assert_eq!(&read(f), before);
        }
    }
}

#[test]
fn retry_and_stop_keep_store_only_semantics_and_exact_generation() {
    let mut f = Fixture::new(true, false);
    let pending = request(&mut f);
    answer(&mut f);
    let job = PackDownloadJob {
        peer: f.peer.clone(),
        pack_id: *f.pack.id(),
        sticker_hash: pending.sticker_hash,
        token: pending.token,
    };
    f.store
        .defer_sticker_pack_download(&job, PackDownloadFailure::Fetch, 1_000)
        .unwrap();
    let sealed = f.store.sealed_device().unwrap();
    let before = read(&mut f);
    assert_stale(&mut f, &before);
    assert!(apply(&mut f, Kind::Retry, Some(pending.token))
        .unwrap()
        .is_none());
    let retried = read(&mut f);
    assert_eq!(retried.state, State::Downloading);
    let generation = retried.generation.unwrap();
    assert_ne!(generation, pending.token);
    assert!(matches!(
        apply(&mut f, Kind::StopWaiting, Some(pending.token)),
        Err(Error::RequestChanged)
    ));
    assert_eq!(retried.retry_at_ms, Some(0));
    assert!(matches!(
        apply(&mut f, Kind::Fresh, Some(generation)),
        Err(Error::Unpublished)
    ));
    assert!(apply(&mut f, Kind::StopWaiting, Some(generation))
        .unwrap()
        .is_none());
    assert_eq!(read(&mut f).state, State::New);
    assert!(matches!(
        apply(&mut f, Kind::StopWaiting, Some(pending.token)),
        Err(Error::RequestChanged)
    ));
    assert_eq!(f.store.sealed_device().unwrap(), sealed);
    assert!(f.store.due_outbound(i64::MAX).unwrap().is_empty());
}

#[test]
fn actions_refuse_unknown_blocked_own_peer_and_cleared_selection_before_mutation() {
    let mut f = Fixture::new(true, false);
    let pending = request(&mut f);
    let own = f.store.owner_peer_id().to_owned();
    for kind in [Kind::Request, Kind::Retry, Kind::Fresh, Kind::StopWaiting] {
        let action = Action {
            kind,
            generation: Some(pending.token),
        };
        for (peer, expected) in [(own.as_str(), "ownPeer"), ("unknown", "unknownPeer")] {
            assert_eq!(
                prepare(&mut f.store, &SEAL, peer, &f.key, action)
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
        }
        f.store.set_blocked(&f.peer, true).unwrap();
        assert!(matches!(
            apply(&mut f, kind, Some(pending.token)),
            Err(Error::Media(sticker_media::Error::Blocked))
        ));
        f.store.set_blocked(&f.peer, false).unwrap();
    }
    assert!(f.store.pending_sticker_pack(&f.peer, f.pack.id()).unwrap() == Some(pending));
    f.store.clear_chat_pair(&f.peer).unwrap();
    assert!(matches!(
        apply(&mut f, Kind::Request, None),
        Err(Error::Media(sticker_media::Error::UnknownMessage))
    ));
    assert!(f.store.due_outbound(i64::MAX).unwrap().is_empty());
}

#[test]
fn duplicate_initial_action_reconciles_saved_intent_without_opening_mls() {
    let mut f = Fixture::new(true, false);
    request(&mut f);
    f.store.save_sticker_pack(&SEAL, &f.pack).unwrap();
    let sealed = f.store.sealed_device().unwrap();
    assert!(apply(&mut f, Kind::Request, None).unwrap().is_none());
    assert_eq!(read(&mut f).state, State::Saved);
    assert!(f
        .store
        .pending_sticker_pack(&f.peer, f.pack.id())
        .unwrap()
        .is_none());
    assert_eq!(f.store.sealed_device().unwrap(), sealed);
}
