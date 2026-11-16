use super::*;
use crate::store::Store;
use dyapp_identity::{Identity, SignedRecord};
use dyapp_profile::Profile;
use std::sync::{atomic::Ordering, Arc};

fn make_session(owner: &Identity) -> Session {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/mailbox-failure-tests")
        .join(format!("{:016x}", getrandom::u64().unwrap()));
    std::fs::create_dir_all(&root).unwrap();
    Session::open_or_create(
        Store::open(&root.join("client.db"), owner).unwrap(),
        owner,
        &Identity::generate(),
        &[5; 32],
    )
    .unwrap()
}

fn refused() -> Error {
    Error::Client(crate::Error::Status(Status::Refused))
}

async fn original(session: &Session, node: &mut Node) {
    assert!(matches!(
        session.classify_mailbox_failure(node, refused()).await,
        MailboxFailure::Original(Error::Client(crate::Error::Status(Status::Refused)))
    ));
}

#[tokio::test]
async fn actual_readback_requires_both_refusal_and_newer_verified_removal() {
    let owner = Identity::generate();
    let session = make_session(&owner);
    session.store().note_device_listed(1).unwrap();
    let listed = Profile {
        version: 2,
        devices: vec![session.certificate().unwrap()],
        ..Default::default()
    };
    let server = super::server::Server::new(listed.sign(&owner));
    let mut node = Node::connect(
        server.address.clone(),
        Arc::new(dyapp_p2p_net::Protocols::new("").unwrap()),
    )
    .await
    .unwrap();
    original(&session, &mut node).await;
    // The observed listing advances the witness, so equal and older absence prove nothing.
    for version in [1, 2] {
        *server.record.lock().unwrap() = Some(
            Profile {
                version,
                ..Default::default()
            }
            .sign(&owner),
        );
        original(&session, &mut node).await;
    }
    *server.record.lock().unwrap() = Some(
        Profile {
            version: 3,
            ..Default::default()
        }
        .sign(&owner),
    );
    assert!(matches!(
        session.classify_mailbox_failure(&mut node, refused()).await,
        MailboxFailure::ConfirmedRevocation
    ));
    original_errors_do_not_read(&session, &mut node, &server).await;
}

async fn original_errors_do_not_read(
    session: &Session,
    node: &mut Node,
    server: &super::server::Server,
) {
    let reads = server.reads.load(Ordering::SeqCst);
    for error in [
        Error::Revoked,
        Error::Invalidated,
        Error::Client(crate::Error::Status(Status::Denied)),
    ] {
        let text = error.to_string();
        let MailboxFailure::Original(retained) =
            session.classify_mailbox_failure(node, error).await
        else {
            panic!("an original failure minted confirmed revocation");
        };
        assert_eq!(retained.to_string(), text);
    }
    assert_eq!(server.reads.load(Ordering::SeqCst), reads);
}

#[tokio::test]
async fn missing_foreign_tampered_and_withdrawn_profiles_never_confirm_cleanup() {
    let owner = Identity::generate();
    let session = make_session(&owner);
    session.store().note_device_listed(1).unwrap();
    let removed = Profile {
        version: 2,
        ..Default::default()
    };
    let valid = removed.sign(&owner);
    let mut tampered = valid.clone();
    tampered.signature[0] ^= 1;
    let server = super::server::Server::new(valid);
    let mut node = Node::connect(
        server.address.clone(),
        Arc::new(dyapp_p2p_net::Protocols::new("").unwrap()),
    )
    .await
    .unwrap();
    let replies: [Option<SignedRecord>; 4] = [
        None,
        Some(removed.sign(&Identity::generate())),
        Some(tampered),
        Some(Profile::tombstone(2, 0, 0).sign(&owner)),
    ];
    for reply in replies {
        *server.record.lock().unwrap() = reply;
        original(&session, &mut node).await;
    }
    let never_listed = make_session(&owner);
    *server.record.lock().unwrap() = Some(removed.sign(&owner));
    original(&never_listed, &mut node).await;
}
