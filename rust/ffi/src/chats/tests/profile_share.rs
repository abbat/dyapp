use super::favorites::Fixture;
use super::offline::{establish, snapshot};
use super::*;

#[test]
fn native_share_queues_one_canonical_link_and_retains_its_exact_committed_bytes() {
    let f = Fixture::new();
    let target = establish(&f);
    let shared = f.key.peer_id();
    let result = f
        .chats
        .submit_profile_share(target.clone(), shared.clone())
        .unwrap();
    let ChatSubmission::Queued { id, .. } = result else {
        panic!("offline native share must queue");
    };
    let link = dyapp_identity::profile_link::profile_link(&shared).unwrap();
    let before = snapshot(&f);
    let f = f.reopen();
    assert_eq!(snapshot(&f), before);
    let messages = f.chats.chat_history(target).unwrap().messages;
    let matching: Vec<_> = messages.iter().filter(|row| row.id == id).collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].text.as_deref(), Some(link.as_str()));
    assert!(!matching[0].forwarded);
}

#[test]
fn native_share_refuses_invalid_same_own_and_missing_targets_before_network_or_mutation() {
    let f = Fixture::new();
    let target = establish(&f);
    let shared = f.key.peer_id();
    let before = snapshot(&f);
    let history = f.chats.chat_history(target.clone()).unwrap().messages;
    for (to, link) in [
        (target.clone(), "invalid".into()),
        (target.clone(), "AB".repeat(32)),
        (target.clone(), target.clone()),
        (shared.clone(), target.clone()),
        (Identity::try_generate().unwrap().peer_id(), shared),
    ] {
        assert_eq!(
            f.chats
                .submit_profile_share(to, link)
                .unwrap_err()
                .to_string(),
            "invalidTarget"
        );
        assert_eq!(snapshot(&f), before);
        assert_eq!(
            f.chats.chat_history(target.clone()).unwrap().messages,
            history
        );
    }
}

fn revoke_share_admission(f: &Fixture, target: &str, shared: &str, state: &str) {
    let session = f.chats.session.lock().unwrap();
    assert!(session.prepare_profile_share(target, shared).is_ok());
    let store = session.store();
    match state {
        "closed" => {
            let mut persisted = Store::open(
                &std::path::Path::new(&f.dir).join("client.db"),
                &f.key.identity,
            )
            .unwrap();
            persisted
                .apply_interaction(
                    shared,
                    target,
                    &[3],
                    dyapp_client::interactions::Interaction {
                        kind: dyapp_client::interactions::Kind::Unlike,
                        event_id: [2; 16],
                        hlc: 2_000,
                    },
                )
                .unwrap();
        }
        "blocked" => store.set_blocked(target, true).unwrap(),
        _ => {
            let mut own = store.profile_draft().unwrap().unwrap();
            own.devices.clear();
            store.save_profile_draft(&own).unwrap();
        }
    }
}

#[test]
fn native_share_rechecks_closed_blocked_and_unlisted_state_without_staging() {
    for state in ["closed", "blocked", "unlisted"] {
        let f = Fixture::new();
        let target = establish(&f);
        let shared = f.key.peer_id();
        revoke_share_admission(&f, &target, &shared, state);
        let before = snapshot(&f);
        let expected = match state {
            "closed" => "invalidTarget",
            "blocked" => "blocked",
            _ => "unpublished",
        };
        assert_eq!(
            f.chats
                .submit_profile_share(target, shared)
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(snapshot(&f), before);
    }
}
