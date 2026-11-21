use super::*;

#[test]
fn target_projection_is_read_only_and_accepts_only_other_open_chats() {
    let f = Fixture::new();
    let before: Vec<_> = f
        .store
        .due_outbound(0)
        .unwrap()
        .iter()
        .map(|item| item.as_bytes().to_vec())
        .collect();
    for phase in 1..=4 {
        let request = (phase < 3).then_some(vec![8u8; 16]);
        f.db.execute(
            "UPDATE `conversations` SET `state` = ?1, `request_id` = ?2 WHERE `peer_id` = ?3",
            rusqlite::params![phase, request, &f.target],
        )
        .unwrap();
        let expected = if phase == 3 {
            vec![f.target.clone()]
        } else {
            Vec::new()
        };
        assert_eq!(targets(&f.store, f.request()).unwrap(), expected);
        let after: Vec<_> = f
            .store
            .due_outbound(0)
            .unwrap()
            .iter()
            .map(|item| item.as_bytes().to_vec())
            .collect();
        assert_eq!(after, before);
        assert_eq!(f.store.current_chat_messages(&f.source).unwrap().len(), 1);
    }
}

#[test]
fn full_source_key_is_validated_even_without_any_available_destination() {
    let f = Fixture::new();
    f.store.set_blocked(&f.target, true).unwrap();
    for source in [
        Source {
            group: &[9],
            ..f.request()
        },
        Source {
            author: &f.target,
            ..f.request()
        },
        Source {
            id: &[3; 16],
            ..f.request()
        },
        Source {
            id: &[3],
            ..f.request()
        },
    ] {
        assert!(matches!(
            targets(&f.store, source),
            Err(Error::UnknownMessage)
        ));
    }
    assert_eq!(
        targets(&f.store, f.request()).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn own_unknown_and_blocked_sources_cannot_offer_destinations() {
    let f = Fixture::new();
    let unknown = "ef".repeat(32);
    for (peer, own) in [(f.store.owner_peer_id(), true), (unknown.as_str(), false)] {
        let error = targets(
            &f.store,
            Source {
                peer,
                ..f.request()
            },
        )
        .unwrap_err();
        assert!(matches!(
            (error, own),
            (Error::OwnPeer, true) | (Error::UnknownPeer, false)
        ));
    }
    f.store.set_blocked(&f.source, true).unwrap();
    assert!(matches!(
        targets(&f.store, f.request()),
        Err(Error::Blocked)
    ));
}

#[test]
fn clearing_the_source_retires_choices_and_unreadable_state_does_not_become_an_empty_list() {
    let mut f = Fixture::new();
    assert_eq!(
        targets(&f.store, f.request()).unwrap(),
        vec![f.target.clone()]
    );
    f.store.clear_chat_pair(&f.source).unwrap();
    assert!(matches!(
        targets(&f.store, f.request()),
        Err(Error::UnknownMessage)
    ));
    f.db.execute(
        "ALTER TABLE `conversations` RENAME TO `unavailable_conversations`",
        [],
    )
    .unwrap();
    assert!(matches!(
        targets(&f.store, f.request()),
        Err(Error::Store(_))
    ));
    f.db.execute(
        "ALTER TABLE `unavailable_conversations` RENAME TO `conversations`",
        [],
    )
    .unwrap();
    assert!(matches!(
        targets(&f.store, f.request()),
        Err(Error::UnknownMessage)
    ));
}
