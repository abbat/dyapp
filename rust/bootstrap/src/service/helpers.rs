use crate::config::Turn;
use dyapp_identity::{Domain, SignedRecord};
use dyapp_p2p_net::proto::{
    MailboxResponse, NodeResponse, ProfileResponse, Status, TurnCredentials,
};
use prost::Message;

/// Checks a request signed by a mailbox's device key or an owner's identity key and passing
/// `check` (its nonce); returns the hash of the signing key (the mailbox address or the peer ID)
/// and the decoded request.
pub(super) fn owner_request<M: Message + Default>(
    record: &SignedRecord,
    domain: Domain,
    check: impl Fn(&M) -> bool,
) -> Result<([u8; 32], M), ()> {
    record.verify(domain).map_err(|_| ())?;
    let request = M::decode(record.payload.as_slice()).map_err(|_| ())?;
    if !check(&request) {
        return Err(());
    }
    let key: [u8; 32] = record.public_key.as_slice().try_into().map_err(|_| ())?;
    Ok((dyapp_identity::key_hash(&key), request))
}

pub(super) fn status(status: Status) -> MailboxResponse {
    MailboxResponse {
        status: status.into(),
        ..MailboxResponse::default()
    }
}

pub(super) fn profile_too_large(max: usize) -> ProfileResponse {
    ProfileResponse {
        max_bytes: max as u64,
        ..reply(Status::TooLarge, None)
    }
}

pub(super) fn reply(status: Status, record: Option<SignedRecord>) -> ProfileResponse {
    ProfileResponse {
        status: status.into(),
        record,
        ..ProfileResponse::default()
    }
}

pub(super) fn node_status(status: Status) -> NodeResponse {
    NodeResponse {
        status: status.into(),
        ..NodeResponse::default()
    }
}

/// A stable pseudonym of an IP group: the username leaks no address, and rotating `turn.secret`
/// renews every pseudonym.
pub(super) fn turn_user(turn: &Turn, group: &str) -> Result<String, hmac::digest::InvalidLength> {
    Ok(hex(&turn
        .secret
        .hmac_sha1(format!("ip-group:{group}").as_bytes())?[..8]))
}

/// TURN REST credentials as coturn's `use-auth-secret` checks them: username
/// `<expiry>:<user>`, password base64(HMAC-SHA1(secret, username)).
pub(super) fn turn_credentials(
    turn: &Turn,
    user: &str,
    now: u64,
) -> Result<TurnCredentials, hmac::digest::InvalidLength> {
    use base64::Engine;
    let expires = now.saturating_add(u64::from(turn.credential_minutes).saturating_mul(60));
    let username = format!("{expires}:{user}");
    let mac = turn.secret.hmac_sha1(username.as_bytes())?;
    Ok(TurnCredentials {
        password: base64::engine::general_purpose::STANDARD.encode(mac),
        username,
        urls: turn.urls.clone(),
        expires,
    })
}

pub(super) fn hex(bytes: &[u8]) -> String {
    dyapp_identity::hex(bytes)
}
