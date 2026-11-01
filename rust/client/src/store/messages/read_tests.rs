use super::{Body, InboxKey, Store, VerifiedChatSender};
use dyapp_identity::Identity;
use dyapp_messaging::hlc::Timestamp;

fn deliver(
    store: &mut Store,
    envelope: u8,
    group: &[u8],
    author: &str,
    device: &[u8; 32],
    body: Body,
) {
    let inbox = InboxKey {
        mailbox: [1; 32],
        id: [envelope; 16],
    };
    assert!(store
        .stage_chat_inbound(
            b"mls",
            &inbox,
            VerifiedChatSender {
                group,
                author,
                device,
            },
            &body,
            1_000,
        )
        .unwrap());
}

fn text(id: u8, time: Option<u64>) -> Body {
    Body::Text {
        id: [id; 16],
        hlc: time.map(|ms| Timestamp::new(ms, 0).into()),
        text: vec![id],
    }
}

#[test]
fn pair_history_orders_legacy_and_timed_current_messages_across_groups() {
    let path = crate::store::tests::temp_db_path();
    let owner = Identity::generate();
    let own = owner.peer_id();
    let peer = Identity::generate().peer_id();
    let other = Identity::generate().peer_id();
    let (a, b, c, device) = ([3], [4], [5], [6; 32]);
    let mut store = Store::open(&path, &owner).unwrap();
    for (group, author) in [(&a[..], &peer), (&b[..], &peer), (&c[..], &other)] {
        crate::store::tests::catalog_group(&mut store, group, author);
    }
    deliver(&mut store, 1, &b, &peer, &device, text(1, None));
    deliver(&mut store, 2, &b, &own, &device, text(2, None));
    deliver(&mut store, 3, &a, &peer, &device, text(3, Some(200)));
    deliver(&mut store, 4, &b, &peer, &device, text(3, Some(200)));
    deliver(&mut store, 5, &a, &own, &device, text(4, Some(150)));
    deliver(&mut store, 6, &c, &other, &device, text(5, None));

    let history = store.current_chat_messages(&peer).unwrap();
    assert_eq!(history.len(), 5);
    assert_eq!(
        history
            .iter()
            .map(|message| (message.key.group.as_slice(), message.key.id[0]))
            .collect::<Vec<_>>(),
        vec![
            (&b[..], 1),
            (&b[..], 2),
            (&a[..], 4),
            (&a[..], 3),
            (&b[..], 3)
        ]
    );
    assert!(history[..2]
        .iter()
        .all(|message| message.original_hlc.is_none()));
    assert_eq!(history[2].original_hlc, Some(Timestamp::new(150, 0)));
    assert!(history[0].unread && !history[1].unread && !history[2].unread);
    assert!(history[3].unread && history[4].unread);
    assert_eq!(store.current_chat_messages(&other).unwrap().len(), 1);

    let edit = Body::Edit {
        target: [3; 16],
        operation: [7; 16],
        hlc: Timestamp::new(300, 0).into(),
        text: b"edited".to_vec(),
    };
    let delete = Body::Delete {
        target: [3; 16],
        operation: [8; 16],
        hlc: Timestamp::new(400, 0).into(),
    };
    deliver(&mut store, 7, &a, &peer, &device, edit);
    deliver(&mut store, 8, &b, &peer, &device, delete);
    deliver(&mut store, 9, &a, &peer, &device, text(9, Some(100)));
    drop(store);

    let mut store = Store::open(&path, &owner).unwrap();
    let history = store.current_chat_messages(&peer).unwrap();
    assert_eq!(history.len(), 6);
    assert_eq!(history[2].key.id, [9; 16]);
    assert_eq!(history[3].key.author, own);
    assert_eq!(history[4].current.text.as_deref(), Some(&b"edited"[..]));
    assert!(history[4].current.edited && history[4].unread);
    assert!(history[5].current.deleted && history[5].unread);
    assert_eq!(history[4].original_hlc, Some(Timestamp::new(200, 0)));
    assert_eq!(history[5].original_hlc, Some(Timestamp::new(200, 0)));
    assert_eq!(store.mark_chat_read(&peer).unwrap(), 4);
    assert!(store
        .current_chat_messages(&peer)
        .unwrap()
        .iter()
        .all(|message| !message.unread));
    assert_eq!(store.clear_chat_pair(&peer).unwrap(), 6);
    assert!(store.current_chat_messages(&peer).unwrap().is_empty());
    assert_eq!(store.current_chat_messages(&other).unwrap().len(), 1);
}
