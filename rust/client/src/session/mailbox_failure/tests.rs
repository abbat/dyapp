use super::*;
use crate::store::Store;
use dyapp_identity::{Identity, SignedRecord};
use dyapp_profile::Profile;
use std::sync::{atomic::Ordering, Arc};

fn make_session(owner: &Identity) -> (Session, Identity) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/mailbox-failure-tests")
        .join(format!("{:016x}", getrandom::u64().unwrap()));
    std::fs::create_dir_all(&root).unwrap();
    let device = Identity::generate();
    let session = Session::open_or_create(
        Store::open(&root.join("client.db"), owner).unwrap(),
        owner,
        &device,
        &[5; 32],
    )
    .unwrap();
    (session, device)
}

async fn connect(server: &super::server::Server) -> Node {
    Node::connect(
        server.address.clone(),
        Arc::new(dyapp_p2p_net::Protocols::new("").unwrap()),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn probe_reads_one_envelope_without_acknowledging_or_applying_it() {
    let owner = Identity::generate();
    let (session, device) = make_session(&owner);
    let original_state = (
        session.store().sealed_device().unwrap(),
        session.store().link_state().unwrap(),
    );
    session.store().note_device_listed(1).unwrap();
    let server = super::server::Server::new(
        Profile {
            version: 2,
            ..Default::default()
        }
        .sign(&owner),
    );
    let mut node = connect(&server).await;
    assert!(!session
        .probe_mailbox_revocation(&mut node, &device)
        .await
        .unwrap());
    assert_eq!(server.reads.load(Ordering::SeqCst), 0);
    assert_eq!(server.mailbox.fetches.load(Ordering::SeqCst), 1);
    assert_eq!(server.mailbox.acknowledgements.load(Ordering::SeqCst), 0);
    assert!(server.mailbox.pending.load(Ordering::SeqCst));
    let retained = node.fetch_page(&device, None, 1).await.unwrap();
    assert_eq!(retained.envelopes.len(), 1);
    assert_eq!(retained.envelopes[0].id, vec![9; 16]);
    assert_eq!(server.mailbox.acknowledgements.load(Ordering::SeqCst), 0);
    assert_eq!(
        (
            session.store().sealed_device().unwrap(),
            session.store().link_state().unwrap()
        ),
        original_state
    );
}

#[tokio::test]
async fn probe_requires_actual_refusal_and_verified_removal() {
    let owner = Identity::generate();
    let (session, device) = make_session(&owner);
    session.store().note_device_listed(1).unwrap();
    let server = super::server::Server::new(
        Profile {
            version: 2,
            ..Default::default()
        }
        .sign(&owner),
    );
    let mut node = connect(&server).await;
    server
        .mailbox
        .status
        .store(Status::Refused.into(), Ordering::SeqCst);
    assert!(session
        .probe_mailbox_revocation(&mut node, &device)
        .await
        .unwrap());
    *server.record.lock().unwrap() = None;
    assert!(matches!(
        session.probe_mailbox_revocation(&mut node, &device).await,
        Err(Error::Client(crate::Error::Status(Status::Refused)))
    ));
    let reads = server.reads.load(Ordering::SeqCst);
    server
        .mailbox
        .status
        .store(Status::Denied.into(), Ordering::SeqCst);
    assert!(matches!(
        session.probe_mailbox_revocation(&mut node, &device).await,
        Err(Error::Client(crate::Error::Status(Status::Denied)))
    ));
    assert_eq!(server.reads.load(Ordering::SeqCst), reads);
    assert_eq!(server.mailbox.acknowledgements.load(Ordering::SeqCst), 0);
    assert!(server.mailbox.pending.load(Ordering::SeqCst));
}

#[tokio::test]
async fn probe_rejects_replaced_device_before_network_io() {
    let owner = Identity::generate();
    let (session, _) = make_session(&owner);
    let server = super::server::Server::new(Profile::default().sign(&owner));
    let mut node = connect(&server).await;
    assert!(matches!(
        session
            .probe_mailbox_revocation(&mut node, &Identity::generate())
            .await,
        Err(Error::Store(crate::store::Error::WrongIdentity))
    ));
    assert_eq!(server.mailbox.fetches.load(Ordering::SeqCst), 0);
    assert_eq!(server.reads.load(Ordering::SeqCst), 0);
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
    let (session, _) = make_session(&owner);
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
    let (session, _) = make_session(&owner);
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
    let (never_listed, _) = make_session(&owner);
    *server.record.lock().unwrap() = Some(removed.sign(&owner));
    original(&never_listed, &mut node).await;
}
