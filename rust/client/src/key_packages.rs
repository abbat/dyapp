//! Signed device `KeyPackage` publication and bounded allocation-slot fetching.

use super::{mailbox, now, ok, unhex};
use crate::{Error, Node};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, key_package_request, Status};
use dyapp_p2p_net::REPLICAS;
use dyapp_profile::VerifiedProfile;
use prost::Message as _;

// Expiry of a published KeyPackage record, below the node's 30-day bound for clock skew.
const KEY_PACKAGE_RECORD_SECONDS: u64 = dyapp_messaging::mls::KEY_PACKAGE_LIFETIME_SECONDS - 3_600;

/// A `KeyPackage` record of the device with mailbox key `device`, owned by `owner` (hex Peer ID).
pub(crate) fn sign_key_package(
    device: &Identity,
    owner: &str,
    package: Vec<u8>,
    last_resort: bool,
    revision: u32,
) -> Result<SignedRecord, Error> {
    let record = proto::KeyPackageRecord {
        owner: unhex(owner)?,
        mailbox: mailbox(&device.public_key()).to_vec(),
        package,
        expires: now().saturating_add(KEY_PACKAGE_RECORD_SECONDS),
        last_resort,
        revision,
    };
    Ok(device.sign(Domain::KeyPackage, record.encode_to_vec()))
}

impl Node {
    /// Publishes a `KeyPackage` of the device whose mailbox key is `device`, owned by `owner`
    /// (hex Peer ID).
    ///
    /// # Errors
    /// Returns [`Error::Input`] for an invalid owner Peer ID, or the request,
    /// deadline, status and reply errors of [`Self::publish_signed_key_package`].
    pub async fn publish_key_package(
        &mut self,
        device: &Identity,
        owner: &str,
        package: Vec<u8>,
        last_resort: bool,
        revision: u32,
    ) -> Result<(), Error> {
        let signed = sign_key_package(device, owner, package, last_resort, revision)?;
        self.publish_signed_key_package(signed).await
    }

    /// Publishes a signed `KeyPackage` record; the node takes a repeat of a stored one as done.
    ///
    /// # Errors
    /// Returns request/deadline errors, [`Error::Status`] for rejection, or
    /// [`Error::Reply`] for an unknown status.
    pub async fn publish_signed_key_package(&mut self, signed: SignedRecord) -> Result<(), Error> {
        let response = self
            .keypackage(key_package_request::Request::Publish(signed))
            .await?;
        ok(response.status)
    }

    /// Takes one `KeyPackage` of the device with `mailbox` in `owner`'s profile, trying each
    /// allocation slot in turn; ordinary ones are consumed, a last-resort one stays.
    ///
    /// # Errors
    /// Returns [`Error::Input`] for an invalid owner Peer ID, request/deadline
    /// errors, the last rejected slot's status, or [`Error::Reply`] for missing,
    /// malformed, wrongly signed or wrong-device records.
    pub async fn fetch_key_package(
        &mut self,
        owner: &VerifiedProfile,
        mailbox: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let mut last = Error::Status(Status::NotFound);
        for slot in 0..u32::from(REPLICAS) {
            let request = proto::GetKeyPackage {
                owner: unhex(&owner.peer_id)?,
                mailbox: mailbox.to_vec(),
                profile_version: owner.profile.version,
                slot,
            };
            let response = self
                .keypackage(key_package_request::Request::Fetch(request))
                .await?;
            if let Err(error) = ok(response.status) {
                last = error;
                continue;
            }
            let signed = response.record.ok_or(Error::Reply("no record"))?;
            let issuer = signed
                .verify(Domain::KeyPackage)
                .map_err(|_| Error::Reply("bad KeyPackage signature"))?;
            let record = proto::KeyPackageRecord::decode(signed.payload.as_slice())
                .map_err(|_| Error::Reply("bad KeyPackage record"))?;
            if unhex(&issuer)? != mailbox || record.mailbox != mailbox {
                return Err(Error::Reply("KeyPackage of another device"));
            }
            return Ok(record.package);
        }
        Err(last)
    }
}
