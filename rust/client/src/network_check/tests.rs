use super::*;

fn contact_profile() -> dyapp_profile::VerifiedProfile {
    let owner = dyapp_identity::Identity::from_secret(&[2; 32]);
    dyapp_profile::verify(
        &dyapp_profile::Profile {
            version: 1,
            ..Default::default()
        }
        .sign(&owner),
    )
    .unwrap()
}

#[test]
fn every_activation_invalidates_shared_contact_tickets_including_switch_back() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let owner = dyapp_identity::Identity::from_secret(&[1; 32]);
    let store = crate::store::Store::open(&dir.join("client.db"), &owner).unwrap();
    let fetched = contact_profile();
    let context = crate::contact_verification::shared(&dir).unwrap();
    let another = crate::contact_verification::shared(&dir.join(".")).unwrap();
    let checks = Checks::default();
    let ticket = context
        .lock()
        .unwrap()
        .begin(&store, &fetched.peer_id)
        .unwrap();
    connected(&checks, &dir, &draft("A"));
    checks.activate(&dir).unwrap();
    assert!(matches!(
        another
            .lock()
            .unwrap()
            .confirm(&store, ticket, fetched.clone()),
        Err(crate::contact_verification::Error::ContextChanged)
    ));
    let ticket = context
        .lock()
        .unwrap()
        .begin(&store, &fetched.peer_id)
        .unwrap();
    for name in ["B", "A"] {
        connected(&checks, &dir, &draft(name));
        checks.activate(&dir).unwrap();
    }
    assert!(matches!(
        another
            .lock()
            .unwrap()
            .confirm(&store, ticket, fetched.clone()),
        Err(crate::contact_verification::Error::ContextChanged)
    ));
    let ticket = context
        .lock()
        .unwrap()
        .begin(&store, &fetched.peer_id)
        .unwrap();
    connected(&checks, &dir, &draft("A"));
    checks.activate(&dir).unwrap();
    assert!(matches!(
        another.lock().unwrap().confirm(&store, ticket, fetched),
        Err(crate::contact_verification::Error::ContextChanged)
    ));
}

#[test]
fn failed_active_save_invalidates_contacts_but_preserves_activation_retry() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let owner = dyapp_identity::Identity::from_secret(&[1; 32]);
    let store = crate::store::Store::open(&dir.join("client.db"), &owner).unwrap();
    let fetched = contact_profile();
    let context = crate::contact_verification::shared(&dir).unwrap();
    let ticket = context
        .lock()
        .unwrap()
        .begin(&store, &fetched.peer_id)
        .unwrap();
    let checks = Checks::default();
    connected(&checks, &dir, &draft("A"));
    // Explicit filesystem state makes rename fail; no permissions or timing assumptions.
    std::fs::create_dir(dir.join(Settings::Active.file_name())).unwrap();
    assert!(matches!(checks.activate(&dir), Err(Error::Failed(_))));
    assert!(matches!(
        context
            .lock()
            .unwrap()
            .confirm(&store, ticket, fetched.clone()),
        Err(crate::contact_verification::Error::ContextChanged)
    ));
    std::fs::rename(
        dir.join(Settings::Active.file_name()),
        dir.join("failed-save-target"),
    )
    .unwrap();
    assert_eq!(checks.activate(&dir), Ok(draft("A")));
    let ticket = context
        .lock()
        .unwrap()
        .begin(&store, &fetched.peer_id)
        .unwrap();
    context
        .lock()
        .unwrap()
        .confirm(&store, ticket, fetched)
        .unwrap();
}

#[test]
fn a_save_error_after_replacement_cannot_restore_the_previous_ticket() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let owner = dyapp_identity::Identity::from_secret(&[1; 32]);
    let store = crate::store::Store::open(&dir.join("client.db"), &owner).unwrap();
    let fetched = contact_profile();
    let context = crate::contact_verification::shared(&dir).unwrap();
    let ticket = context
        .lock()
        .unwrap()
        .begin(&store, &fetched.peer_id)
        .unwrap();
    let checks = Checks::default();
    connected(&checks, &dir, &draft("B"));
    let result = checks.activate_with(&dir, |dir, network| {
        assert!(matches!(
            context.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        network::save(dir, Settings::Active, network)?;
        Err(std::io::Error::other(
            "controlled failure after replacement",
        ))
    });
    assert!(matches!(result, Err(Error::Failed(_))));
    assert_eq!(
        network::load(&dir, Settings::Active).unwrap(),
        Some(draft("B"))
    );
    assert!(matches!(
        context.lock().unwrap().confirm(&store, ticket, fetched),
        Err(crate::contact_verification::Error::ContextChanged)
    ));
    assert_eq!(checks.activate(&dir), Ok(draft("B")));
}

fn draft(name: &str) -> Network {
    Network {
        seeds: vec!["not an address".into()],
        name: name.into(),
    }
}

// Exercise admission below the verified probe boundary; native tests use an answering swarm.
fn connected(checks: &Checks, dir: &Path, draft: &Network) -> Arc<Cancellation> {
    let attempt = checks.begin(dir, draft).unwrap();
    checks
        .complete(
            &attempt,
            dir.to_owned(),
            draft.clone(),
            Ok(vec![Row::Connected(
                libp2p::identity::Keypair::generate_ed25519()
                    .public()
                    .to_peer_id(),
            )]),
        )
        .unwrap();
    attempt
}

#[test]
fn successful_check_is_directory_bound_consumed_and_cannot_be_restored_by_late_completion() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.path().join("checked");
    let other = test_dir.path().join("other");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let checks = Checks::default();
    let draft = draft("first");
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
    let old = connected(&checks, &dir, &draft);
    assert_eq!(checks.activate(&other), Err(Error::NotChecked));
    assert_eq!(checks.activate(&dir), Ok(draft.clone()));
    assert_eq!(
        network::load(&dir, Settings::Active).unwrap(),
        Some(draft.clone())
    );
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
    assert_eq!(
        checks.complete(&old, dir.clone(), draft, Ok(vec![])),
        Err(Error::Cancelled)
    );
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
}

#[test]
fn identity_transition_and_replacement_check_reject_earlier_completion() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let checks = Checks::default();
    let first = draft("first");
    let old = connected(&checks, &dir, &first);
    checks.clear_checked().unwrap();
    assert!(old.is_cancelled());
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
    let next = checks.begin(&dir, &draft("next")).unwrap();
    assert_eq!(
        checks.complete(&old, dir.clone(), first.clone(), Ok(vec![])),
        Err(Error::Cancelled)
    );
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
    assert!(!next.is_cancelled());
    checks.cancel();
    assert!(next.is_cancelled());
    assert_eq!(
        checks.complete(&next, dir.clone(), first, Ok(vec![])),
        Err(Error::Cancelled)
    );
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
}

#[tokio::test]
async fn failed_recheck_keeps_its_draft_without_reusing_previous_success() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let checks = Checks::default();
    connected(&checks, &dir, &draft("first"));
    let invalid = Network {
        seeds: vec![],
        name: String::new(),
    };
    assert_eq!(
        checks
            .check(
                dir.clone(),
                invalid.clone(),
                Duration::ZERO,
                &Cancellation::new(),
                || Ok(false)
            )
            .await,
        Err(Error::Empty)
    );
    assert_eq!(network::load(&dir, Settings::Draft).unwrap(), Some(invalid));
    assert_eq!(network::load(&dir, Settings::Active).unwrap(), None);
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
    let invalid_name = draft(" city");
    assert!(matches!(
        checks
            .check(
                dir.clone(),
                invalid_name.clone(),
                Duration::ZERO,
                &Cancellation::new(),
                || Ok(false)
            )
            .await,
        Err(Error::InvalidNetwork(_))
    ));
    assert_eq!(
        network::load(&dir, Settings::Draft).unwrap(),
        Some(invalid_name)
    );
}

#[tokio::test]
async fn pre_cancelled_attempt_does_not_write_and_retry_has_independent_registration() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let checks = Checks::default();
    let cancellation = Cancellation::new();
    cancellation.cancel();
    assert_eq!(
        checks
            .check(
                dir.clone(),
                draft(""),
                Duration::ZERO,
                &cancellation,
                || panic!("must not register")
            )
            .await,
        Err(Error::Cancelled)
    );
    assert_eq!(network::load(&dir, Settings::Draft).unwrap(), None);
    assert_eq!(
        checks
            .check(
                dir.clone(),
                draft(""),
                Duration::ZERO,
                &Cancellation::new(),
                || Ok(false)
            )
            .await,
        Ok(vec![Row::Invalid(dyapp_p2p_net::seed::SeedError::Address)])
    );
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
}

#[tokio::test]
async fn registration_signal_cancels_without_sleep_or_network_timing() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let checks = Checks::default();
    let cancellation = Cancellation::new();
    assert_eq!(
        checks
            .check(
                dir.clone(),
                draft(""),
                Duration::ZERO,
                &cancellation,
                || {
                    cancellation.cancel();
                    Ok(false)
                }
            )
            .await,
        Err(Error::Cancelled)
    );
    assert_eq!(
        checks
            .check(
                dir.clone(),
                draft(""),
                Duration::ZERO,
                &Cancellation::new(),
                || Ok(true)
            )
            .await,
        Err(Error::Cancelled)
    );
    assert_eq!(
        network::load(&dir, Settings::Draft).unwrap(),
        Some(draft(""))
    );
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
}

#[tokio::test]
async fn registered_pending_check_is_stopped_by_a_readiness_callback() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    let checks = Checks::default();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let network = Network {
        seeds: vec![listener.local_addr().unwrap().to_string()],
        name: String::new(),
    };
    let cancellation = Cancellation::new();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let running = checks.check(
        dir.clone(),
        network.clone(),
        Duration::from_secs(60),
        &cancellation,
        || {
            entered.send(()).unwrap();
            Ok(false)
        },
    );
    let stop = async {
        ready.await.unwrap();
        checks.cancel();
    };
    let (result, ()) = tokio::time::timeout(
        Duration::from_secs(5),
        Box::pin(async { tokio::join!(running, stop) }),
    )
    .await
    .unwrap();
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(network::load(&dir, Settings::Draft).unwrap(), Some(network));
    assert_eq!(checks.activate(&dir), Err(Error::NotChecked));
}
