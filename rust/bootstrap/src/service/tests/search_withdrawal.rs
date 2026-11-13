//! A search holder's bytes cannot seed or delete ordinary profile storage.
use super::conditional_profile::fixture;
use super::*;
use dyapp_profile::ProfileCondition;

fn hidden_record(service: &Service, owner: &Identity) -> SignedRecord {
    let store =
        dyapp_client::store::Store::open(&service.config.storage.dir.join("client.db"), owner)
            .unwrap();
    let bits = service.config.search.min_profile_pow_bits;
    let profile = Profile {
        version: 1,
        pow_nonce: dyapp_profile::solve(&owner.public_key(), bits, 0, 100_000).unwrap(),
        pow_bits: bits,
        ..Default::default()
    };
    store.save_profile_draft(&profile).unwrap();
    store.hide_profile_draft(owner, &profile).unwrap();
    store.due_search_profile(u64::MAX).unwrap().unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Route {
    Publish,
    Conditional,
    Replica,
    Repair,
    DirectStore,
    Preflight,
}
const ROUTES: [Route; 6] = [
    Route::Publish,
    Route::Conditional,
    Route::Replica,
    Route::Repair,
    Route::DirectStore,
    Route::Preflight,
];

fn submit(service: &Service, operation: profile_request::Request) -> ProfileResponse {
    service
        .profile(
            &replica_peer("holder"),
            ProfileRequest {
                request: Some(operation),
                age_seconds: 0,
            },
        )
        .unwrap()
}

fn replay(service: &Service, record: &SignedRecord, route: Route) {
    let response = match route {
        Route::DirectStore => {
            assert!(matches!(
                service.store.put_profile(record),
                Err(BootstrapError::Profile(
                    dyapp_profile::Error::Signature(_) | dyapp_profile::Error::Invalid(_)
                ))
            ));
            return;
        }
        Route::Repair => service.store_profile(record).unwrap(),
        Route::Preflight => service
            .prepare_profile(&peer("holder"), record.clone())
            .unwrap(),
        Route::Publish => submit(service, profile_request::Request::Publish(record.clone())),
        Route::Conditional => submit(
            service,
            profile_request::Request::ConditionalPublish(record.clone()),
        ),
        Route::Replica => submit(
            service,
            profile_request::Request::ReplicaPut(record.clone()),
        ),
    };
    assert!(
        matches!(status(&response), Status::Denied | Status::Invalid),
        "{route:?}: {:?}",
        status(&response)
    );
}

fn check_replay(legacy: bool, present: bool, route: Route) {
    let (service, owner) = fixture();
    if present {
        service
            .store
            .put_profile(
                &Profile {
                    version: 1,
                    ..Default::default()
                }
                .sign(&owner),
            )
            .unwrap();
    }
    let hidden = if legacy {
        // Exact historical format emitted by pre-purpose clients.
        Profile::tombstone(2, 0, 0).sign(&owner)
    } else {
        hidden_record(&service, &owner)
    };
    replay(&service, &hidden, route);
    let held = service.store.get_profile(&owner.peer_id()).unwrap();
    assert_eq!(
        held.as_ref()
            .map(|r| dyapp_profile::stored_version(r).unwrap()),
        present.then_some(1)
    );
}

#[test]
fn search_and_legacy_hide_cannot_seed_or_delete_through_any_storage_ingress() {
    for (legacy, present) in [(false, false), (false, true), (true, false), (true, true)] {
        for route in ROUTES {
            check_replay(legacy, present, route);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum SearchRoute {
    Rpc,
    Gossip,
    Catchup,
}

fn apply_search(service: &Service, hidden: &SignedRecord, route: SearchRoute) {
    let request = search_request::Request::Withdraw(hidden.clone());
    match route {
        SearchRoute::Rpc => assert_eq!(
            Status::try_from(super::search::search_as(service, &peer("sender"), request).status)
                .unwrap(),
            Status::Ok
        ),
        SearchRoute::Gossip => {
            let bytes = SearchRequest {
                request: Some(request),
            }
            .encode_to_vec();
            let verdict = match service.search_gossip(&bytes) {
                Verdict::Pow(check) => {
                    assert!(service.pow.check(&check));
                    service.search_gossip(&bytes)
                }
                verdict => verdict,
            };
            assert!(matches!(verdict, Verdict::Accept));
        }
        SearchRoute::Catchup => {
            let caught = match service.search_apply(hidden).unwrap() {
                Caught::Pow(check) => {
                    assert!(service.pow.check(&check));
                    service.search_apply(hidden).unwrap()
                }
                caught => caught,
            };
            assert!(matches!(caught, Caught::Stored));
        }
    }
}

#[test]
fn search_withdrawal_round_trips_rpc_gossip_and_catchup_without_ordinary_changes() {
    for route in [SearchRoute::Rpc, SearchRoute::Gossip, SearchRoute::Catchup] {
        let service = search_service();
        let owner = Identity::from_secret(&[7; 32]);
        let hidden = hidden_record(&service, &owner);
        let proof = dyapp_profile::verify_search(&hidden).unwrap().profile;
        let source = Profile {
            version: 1,
            pow_nonce: proof.pow_nonce,
            pow_bits: proof.pow_bits,
            ..Default::default()
        }
        .sign(&owner);
        service.store.put_profile(&source).unwrap();
        apply_search(&service, &hidden, route);
        let reply = super::search::search_as(
            &service,
            &peer("reader"),
            search_request::Request::Get(proto::SearchGet {
                keys: vec![dyapp_identity::sha256(&owner.public_key()).to_vec()],
                puzzle: None,
            }),
        );
        assert_eq!(reply.records, vec![hidden], "{route:?}");
        let replica = search_service();
        apply_search(&replica, &reply.records[0], SearchRoute::Catchup);
        assert_eq!(replica.store.get_profile(&owner.peer_id()).unwrap(), None);
        assert_eq!(
            service.store.get_profile(&owner.peer_id()).unwrap(),
            Some(source)
        );
    }
}

#[test]
fn fresh_conditional_account_deletion_remains_ordinary_authority() {
    let (service, owner) = fixture();
    let original = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    service.store.put_profile(&original).unwrap();
    let deleted = Profile {
        previous: Some(ProfileCondition::observed(Some(&original)).unwrap()),
        ..Profile::tombstone(2, 0, 0)
    }
    .sign(&owner);
    let response = service
        .profile(
            &peer("owner"),
            ProfileRequest {
                request: Some(profile_request::Request::ConditionalPublish(deleted)),
                age_seconds: 0,
            },
        )
        .unwrap();
    assert_eq!(status(&response), Status::Ok);
    let held = service
        .store
        .get_profile(&owner.peer_id())
        .unwrap()
        .unwrap();
    let verified = dyapp_profile::verify(&held).unwrap();
    assert!(verified.profile.deleted);
    assert!(!verified.profile.search_only);
}
