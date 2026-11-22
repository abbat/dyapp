//! Actual native confirmation over independently stored, responding profile nodes.
use super::*;
use dyapp_client::{store::Store, Node};
use dyapp_messaging::mls::Device;
use std::future::Future;
use tokio::runtime::Runtime;

#[path = "verification/controlled.rs"]
mod controlled;
use controlled::Controlled;

fn phase<T>(runtime: &Runtime, label: &str, work: impl Future<Output = T>) -> T {
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), Box::pin(work))
            .await
            .expect(label)
    })
}

fn native<T: Send + 'static>(
    runtime: &Runtime,
    label: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    phase(runtime, label, async {
        tokio::task::spawn_blocking(work).await.unwrap()
    })
}

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

fn server(runtime: &Runtime) -> Multiaddr {
    server_named(runtime, "")
}

fn server_named(runtime: &Runtime, name: &str) -> Multiaddr {
    phase(
        runtime,
        "profile listener readiness",
        start_config_in_protocols(
            |config| {
                config.roles = vec![Role::Store];
                config.limits.requests_per_second = 1000;
                config.limits.min_free_mb = 0;
            },
            Arc::new(dyapp_p2p_net::Protocols::new(name).unwrap()),
        ),
    )
    .0
}

fn profile(version: u64, devices: &[u8]) -> Profile {
    let owner = Identity::from_secret(&[2; 32]);
    let mut certificates: Vec<_> = devices
        .iter()
        .map(|secret| {
            let key = Identity::from_secret(&[*secret; 32]).public_key();
            (
                dyapp_identity::key_hash(&key),
                Device::new(&owner, key, 1).unwrap().certificate().clone(),
            )
        })
        .collect();
    certificates.sort_by_key(|(hash, _)| *hash);
    Profile {
        version,
        devices: certificates
            .into_iter()
            .map(|(_, certificate)| certificate)
            .collect(),
        ..Default::default()
    }
}

fn publish(runtime: &Runtime, address: &Multiaddr, profile: &mut Profile, initial: bool) {
    let mut client = phase(
        runtime,
        "publisher connection",
        Node::connect(
            address.clone(),
            Arc::new(dyapp_p2p_net::Protocols::public()),
        ),
    )
    .unwrap();
    let owner = Identity::from_secret(&[2; 32]);
    if initial {
        phase(
            runtime,
            "initial profile publication",
            client.publish_profile(profile.sign(&owner)),
        )
        .unwrap();
    } else {
        phase(
            runtime,
            "conditional profile publication",
            publish_profile_change(&mut client, &owner, profile),
        )
        .unwrap();
    }
}

fn client(
    rows: Vec<String>,
) -> (
    dyapp_test_support::TestDir,
    std::path::PathBuf,
    Arc<ChatSession>,
) {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    network::save(
        &dir,
        Settings::Active,
        &Network {
            seeds: rows,
            name: String::new(),
        },
    )
    .unwrap();
    let key = |secret| {
        IdentityKey::unwrap(
            Arc::new(TestStore),
            TestStore.wrap(vec![secret; 32]).unwrap(),
        )
        .unwrap()
    };
    let chats = ChatSession::open(
        dir.to_str().unwrap().into(),
        key(1),
        key(9),
        Arc::new(TestStore),
        TestStore.wrap(vec![8; 32]).unwrap(),
    )
    .unwrap();
    (test_dir, dir, chats)
}

fn confirmation(
    runtime: &Runtime,
    chats: &Arc<ChatSession>,
) -> Result<dyapp_ffi::FoundProfile, dyapp_ffi::CoreError> {
    let chats = Arc::clone(chats);
    native(runtime, "native confirmation", move || {
        chats.confirm_contact(Identity::from_secret(&[2; 32]).peer_id())
    })
}

fn confirm(runtime: &Runtime, chats: &Arc<ChatSession>) -> dyapp_ffi::FoundProfile {
    confirmation(runtime, chats).unwrap()
}

#[test]
fn native_confirmation_selects_highest_signed_profile_in_both_seed_orders() {
    let runtime = runtime();
    let (low, high) = (server(&runtime), server(&runtime));
    publish(&runtime, &low, &mut profile(1, &[]), true);
    publish(&runtime, &high, &mut profile(1, &[]), true);
    let mut newest = profile(2, &[3]);
    publish(&runtime, &high, &mut newest, false);
    let signed = dyapp_profile::verify(&newest.sign(&Identity::from_secret(&[2; 32]))).unwrap();
    for rows in [[&low, &high], [&high, &low]] {
        let (_test_dir_165, dir, chats) = client(rows.into_iter().map(seed_row).collect());
        assert_eq!(confirm(&runtime, &chats).version, 2);
        let peer = signed.peer_id.clone();
        assert!(chats.contact_verification(peer.clone()).unwrap().verified);
        let store =
            Store::open_existing(&dir.join("client.db"), &Identity::from_secret(&[1; 32])).unwrap();
        assert!(
            store.verified(&signed).unwrap(),
            "mark used the highest profile's actual devices"
        );
        chats.unverify_contact(peer.clone()).unwrap();
        assert!(!chats.contact_verification(peer).unwrap().verified);
    }
}

#[test]
fn native_device_notice_tracks_network_readback_and_acknowledgement_once() {
    let runtime = runtime();
    let address = server(&runtime);
    publish(&runtime, &address, &mut profile(1, &[3]), true);
    let (_test_dir_185, dir, chats) = client(vec![seed_row(&address)]);
    assert_eq!(confirm(&runtime, &chats).version, 1);
    let peer = Identity::from_secret(&[2; 32]).peer_id();
    publish(&runtime, &address, &mut profile(2, &[3, 4]), false);
    let searcher = dyapp_ffi::ProfileSearcher::new(dir.to_str().unwrap().into()).unwrap();
    let fetched = Arc::clone(&searcher);
    let target = peer.clone();
    assert_eq!(
        native(&runtime, "native profile refresh", move || fetched
            .profiles(vec![target])
            .unwrap())[0]
            .version,
        2
    );
    assert_eq!(
        chats.contact_verification(peer.clone()).unwrap(),
        dyapp_ffi::ContactVerificationState {
            verified: false,
            new_device: true
        }
    );
    chats.acknowledge_contact_devices(peer.clone()).unwrap();
    for _ in 0..2 {
        assert_eq!(
            chats.contact_verification(peer.clone()).unwrap(),
            dyapp_ffi::ContactVerificationState {
                verified: false,
                new_device: false
            }
        );
    }
    assert_eq!(confirm(&runtime, &chats).version, 2);
    assert!(chats.contact_verification(peer).unwrap().verified);
}

#[test]
fn invalid_signed_responses_do_not_mask_a_valid_node_in_either_order() {
    let runtime = runtime();
    let good = server(&runtime);
    publish(&runtime, &good, &mut profile(1, &[3]), true);
    let mut invalid = profile(99, &[]).sign(&Identity::from_secret(&[2; 32]));
    invalid.payload.push(0);
    let mut bad = Controlled::start(&runtime, invalid, false);
    for rows in [[&good, &bad.address], [&bad.address, &good]] {
        let (_test_dir_229, _, chats) = client(rows.into_iter().map(seed_row).collect());
        assert_eq!(confirm(&runtime, &chats).version, 1);
        phase(
            &runtime,
            "invalid responder was queried",
            bad.requests.recv(),
        )
        .unwrap();
    }
}

fn checked(
    runtime: &Runtime,
    dir: &std::path::Path,
    address: &Multiaddr,
    name: &str,
) -> Arc<dyapp_ffi::ConnectionCheck> {
    let checker = dyapp_ffi::ConnectionCheck::new(dir.to_str().unwrap().into()).unwrap();
    let running = Arc::clone(&checker);
    let seeds = vec![seed_row(address)];
    let name = name.to_owned();
    native(runtime, "native seed-check readiness", move || {
        let rows = running
            .run(dyapp_ffi::NetworkSettings { seeds, name }, 30_000)
            .unwrap();
        assert!(rows
            .iter()
            .any(|row| matches!(row, dyapp_ffi::SeedRow::Connected { .. })));
    });
    checker
}

fn activate(
    runtime: &Runtime,
    checker: &Arc<dyapp_ffi::ConnectionCheck>,
) -> dyapp_ffi::NetworkSettings {
    let checker = Arc::clone(checker);
    native(runtime, "checked native network activation", move || {
        checker.activate().unwrap()
    })
}

#[test]
fn a_native_reply_after_real_network_switch_back_cannot_confirm_the_contact() {
    let runtime = runtime();
    let record = profile(1, &[3]).sign(&Identity::from_secret(&[2; 32]));
    let mut blocked = Controlled::start(&runtime, record, true);
    let alternative = server_named(&runtime, "B");
    let (_test_dir_277, dir, chats) = client(vec![seed_row(&blocked.address)]);
    // Finish probe handshakes before holding a profile reply; only the actual activation
    // writes run within the production request deadline, without guessed delays.
    let away = checked(&runtime, &dir, &alternative, "B");
    let back = checked(&runtime, &dir, &blocked.address, "");
    let target = Identity::from_secret(&[2; 32]).peer_id();
    let pending = Arc::clone(&chats);
    let peer = target.clone();
    let task = runtime.spawn_blocking(move || pending.confirm_contact(peer));
    phase(
        &runtime,
        "profile request admitted before switching",
        blocked.requests.recv(),
    )
    .unwrap();
    assert_eq!(activate(&runtime, &away).name, "B");
    assert_eq!(
        network::load(&dir, Settings::Active).unwrap().unwrap().name,
        "B"
    );
    assert_eq!(activate(&runtime, &back).name, String::new());
    assert_eq!(
        network::load(&dir, Settings::Active).unwrap(),
        Some(Network {
            seeds: vec![seed_row(&blocked.address)],
            name: String::new()
        })
    );
    blocked.release.take().unwrap().send(()).unwrap();
    let result = phase(&runtime, "released native confirmation", task).unwrap();
    assert_eq!(result.unwrap_err().to_string(), "notConnected");
    assert!(!chats.contact_verification(target).unwrap().verified);
}

#[test]
fn a_seed_without_a_usable_endpoint_is_disconnected_not_unavailable() {
    let runtime = runtime();
    // Port zero cannot name a listening service; no other process can occupy this endpoint.
    let (_test_dir_315, _, chats) = client(vec!["127.0.0.1:0".into()]);
    let result = native(&runtime, "native unreachable seed", move || {
        chats.confirm_contact(Identity::from_secret(&[2; 32]).peer_id())
    });
    assert_eq!(result.unwrap_err().to_string(), "notConnected");
}

fn terminal_fixture(runtime: &Runtime) -> (Multiaddr, SignedRecord, SignedRecord) {
    let live = server(runtime);
    publish(runtime, &live, &mut profile(1, &[3]), true);
    let owner = Identity::from_secret(&[2; 32]);
    let mut remote = phase(
        runtime,
        "read deletion source",
        Node::connect(live.clone(), Arc::new(dyapp_p2p_net::Protocols::public())),
    )
    .unwrap();
    let (source, _) = phase(
        runtime,
        "signed deletion source",
        remote.get_profile_signed(&owner.peer_id()),
    )
    .unwrap();
    publish(runtime, &live, &mut profile(2, &[3]), false);
    publish(runtime, &live, &mut profile(3, &[3]), false);
    let deletion = Profile {
        previous: Some(dyapp_profile::ProfileCondition::observed(Some(&source)).unwrap()),
        ..Profile::tombstone(2, 0, 0)
    }
    .sign(&owner);
    assert!(dyapp_profile::verify(&deletion).unwrap().profile.deleted);
    let (record, _) = phase(
        runtime,
        "signed live fork",
        remote.get_profile_signed(&owner.peer_id()),
    )
    .unwrap();
    (live, record, deletion)
}

fn terminal_case(
    runtime: &Runtime,
    live: &Multiaddr,
    record: &SignedRecord,
    deletion: &SignedRecord,
    deleted_first: bool,
    held_live: bool,
) {
    let initial = if held_live {
        record.clone()
    } else {
        deletion.clone()
    };
    let mut tombstone = Controlled::start(runtime, initial, false);
    let rows = if deleted_first {
        vec![seed_row(&tombstone.address), seed_row(live)]
    } else {
        vec![seed_row(live), seed_row(&tombstone.address)]
    };
    let (_test_dir_374, _, chats) = client(rows);
    let peer = Identity::from_secret(&[2; 32]).peer_id();
    if held_live {
        assert_eq!(confirm(runtime, &chats).version, 3);
        assert!(chats.contact_verification(peer.clone()).unwrap().verified);
        tombstone.replace(deletion.clone());
    }
    assert_eq!(
        confirmation(runtime, &chats).unwrap_err().to_string(),
        "unpublished"
    );
    assert!(!chats.contact_verification(peer.clone()).unwrap().verified);
    tombstone.stop(runtime);
    assert_eq!(
        confirmation(runtime, &chats).unwrap_err().to_string(),
        "staleProfile"
    );
    assert!(!chats.contact_verification(peer).unwrap().verified);
}

#[test]
fn an_observed_terminal_account_tombstone_cannot_be_hidden_by_a_higher_live_fork() {
    let runtime = runtime();
    let (live, record, deletion) = terminal_fixture(&runtime);
    for (deleted_first, held_live) in [(false, false), (true, false), (false, true)] {
        terminal_case(
            &runtime,
            &live,
            &record,
            &deletion,
            deleted_first,
            held_live,
        );
    }
}

#[test]
fn an_answering_seed_without_a_profile_is_unavailable_not_disconnected() {
    let runtime = runtime();
    let address = server(&runtime);
    let (_test_dir_414, _, chats) = client(vec![seed_row(&address)]);
    let result = native(&runtime, "native missing profile", move || {
        chats.confirm_contact(Identity::from_secret(&[2; 32]).peer_id())
    });
    assert_eq!(result.unwrap_err().to_string(), "unavailable");
}
