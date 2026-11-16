//! Preserve every mailbox-failure classification and compatibility assertion in small helpers.
use dyapp_client::session::{Error, MailboxFailure, Session};
use dyapp_client::Node;
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::Status;

pub(super) async fn original(session: &Session, node: &mut Node) {
    for original in [Error::Revoked, Error::Invalidated] {
        let message = original.to_string();
        let MailboxFailure::Original(retained) =
            session.classify_mailbox_failure(node, original).await
        else {
            panic!("an original error was promoted to confirmed mailbox revocation");
        };
        assert_eq!(retained.to_string(), message);
    }
    // A refusal alone wipes nothing while the newest profile lists the device.
    let refused = || Error::Client(dyapp_client::Error::Status(Status::Refused));
    assert!(matches!(
        session.classify_mailbox_failure(node, refused()).await,
        MailboxFailure::Original(Error::Client(dyapp_client::Error::Status(Status::Refused)))
    ));
    let error = session.revoked_or(node, refused()).await;
    assert!(matches!(error, Error::Client(_)), "{error}");
}

pub(super) async fn confirmed(
    session: &Session,
    node: &mut Node,
    device: &Identity,
    original: Error,
) {
    assert!(matches!(
        session.classify_mailbox_failure(node, original).await,
        MailboxFailure::ConfirmedRevocation
    ));
    let refused = node.fetch(device).await.unwrap_err();
    let error = session.revoked_or(node, refused.into()).await;
    assert!(matches!(error, Error::Revoked), "{error}");
}
