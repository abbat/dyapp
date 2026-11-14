use super::*;

fn dir(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/core-network-check")
        .join(tag)
        .join(dyapp_identity::Identity::generate().peer_id());
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
    let (dir, other) = (dir("owner"), dir("foreign"));
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
    let dir = dir("transition");
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
    let dir = dir("recheck");
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
    let dir = dir("pre-cancel");
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
    let dir = dir("registration");
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
    let dir = dir("running");
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
