//! Admission and lookup for device KeyPackages.

use super::{hex, Peer, Service, SHED_PROFILES};
use crate::config::Role;
use dyapp_identity::{key_hash, sha256, Domain, SignedRecord};
use dyapp_p2p_net::proto::{
    key_package_request, GetKeyPackage, KeyPackageRecord, KeyPackageRequest, KeyPackageResponse,
    Status,
};
use dyapp_profile::{self, DeviceCertificate, VerifiedProfile};
use prost::Message;

const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_AGE_SECONDS: u64 = 30 * 86_400;

fn response(status: Status) -> KeyPackageResponse {
    KeyPackageResponse {
        status: status.into(),
        ..Default::default()
    }
}

impl Service {
    /// Checks a client publication before the node fans it out.
    pub fn prepare_keypackage(
        &self,
        peer: &Peer,
        record: SignedRecord,
    ) -> crate::Result<KeyPackageResponse> {
        self.keypackage_inner(
            peer,
            KeyPackageRequest {
                request: Some(key_package_request::Request::Publish(record)),
            },
            false,
        )
    }

    /// Verifies and stores a replica without per-peer admission limits.
    pub fn store_keypackage(&self, record: &SignedRecord) -> crate::Result<KeyPackageResponse> {
        self.publish_keypackage(record, true)
    }

    /// Handles KeyPackage publication, replication and allocation.
    pub fn keypackage(
        &self,
        peer: &Peer,
        request: KeyPackageRequest,
    ) -> crate::Result<KeyPackageResponse> {
        self.keypackage_inner(peer, request, true)
    }

    fn keypackage_inner(
        &self,
        peer: &Peer,
        request: KeyPackageRequest,
        store: bool,
    ) -> crate::Result<KeyPackageResponse> {
        if !self.config.roles.contains(&Role::Store) {
            return Ok(response(Status::Unsupported));
        }
        let replica = matches!(
            request.request,
            Some(key_package_request::Request::ReplicaPut(_))
        );
        if replica && !self.node_peer(peer) {
            self.strike(peer);
            return Ok(response(Status::Denied));
        }
        let units = if matches!(
            request.request,
            Some(key_package_request::Request::Publish(_))
        ) {
            5
        } else {
            1
        };
        if self.shed(request.encoded_len(), SHED_PROFILES)
            || !if replica {
                self.admit_quiet(peer, units)
            } else {
                self.admit_units(peer, units)
            }
        {
            return Ok(response(Status::RateLimited));
        }
        if self.refused(peer) {
            return Ok(response(Status::Refused));
        }
        match request.request {
            Some(
                key_package_request::Request::Publish(signed)
                | key_package_request::Request::ReplicaPut(signed),
            ) => {
                let answer = self.publish_keypackage(&signed, store)?;
                if answer.status == i32::from(Status::Denied) {
                    self.strike(peer);
                }
                Ok(answer)
            }
            Some(key_package_request::Request::Fetch(get)) => self.fetch_keypackage(&get),
            None => Ok(response(Status::Unsupported)),
        }
    }

    fn current_device(
        &self,
        owner: &[u8],
        mailbox: &[u8],
    ) -> crate::Result<Option<(VerifiedProfile, Vec<u8>)>> {
        let Some(record) = self.store.get_profile(&hex(owner))? else {
            return Ok(None);
        };
        let verified = dyapp_profile::verify(&record)?;
        if verified.profile.deleted {
            return Ok(None);
        }
        let device = verified.profile.devices.iter().find_map(|signed| {
            // `dyapp_profile::verify` checked every certificate's owner signature and keys.
            let certificate = DeviceCertificate::decode(signed.payload.as_slice()).ok()?;
            let key: [u8; 32] = certificate.device_key.as_slice().try_into().ok()?;
            (key_hash(&key).as_slice() == mailbox).then_some(key.to_vec())
        });
        Ok(device.map(|key| (verified, key)))
    }

    fn publish_keypackage(
        &self,
        signed: &SignedRecord,
        store: bool,
    ) -> crate::Result<KeyPackageResponse> {
        if signed.encoded_len() > MAX_RECORD_BYTES {
            return Ok(KeyPackageResponse {
                max_bytes: MAX_RECORD_BYTES as u64,
                ..response(Status::TooLarge)
            });
        }
        if signed.verify(Domain::KeyPackage).is_err() {
            return Ok(response(Status::Denied));
        }
        let Ok(package) = KeyPackageRecord::decode(signed.payload.as_slice()) else {
            return Ok(response(Status::Invalid));
        };
        let (Ok(owner), Ok(device)) = (
            <[u8; 32]>::try_from(package.owner.as_slice()),
            <[u8; 32]>::try_from(signed.public_key.as_slice()),
        ) else {
            return Ok(response(Status::Invalid));
        };
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        if package.mailbox != key_hash(&device)
            || package.package.len() < 5
            || !package.package.starts_with(&[0, 1, 0, 1])
            || package.expires <= now
            || package.expires > now + MAX_AGE_SECONDS
            || (package.last_resort && package.revision == 0)
            || (!package.last_resort && package.revision != 0)
        {
            return Ok(response(Status::Invalid));
        }
        if self.listed(&owner) || self.listed(&package.mailbox) {
            return Ok(response(Status::Refused));
        }
        let Some((_profile, key)) = self.current_device(&owner, &package.mailbox)? else {
            return Ok(response(Status::Denied));
        };
        if key != signed.public_key {
            return Ok(response(Status::Denied));
        }
        if !store {
            return Ok(response(Status::Ok));
        }
        let hash = sha256(&package.package);
        let slot = hash[0] % dyapp_p2p_net::REPLICAS;
        let accepted = self.store.put_keypackage(&package, &hash, slot, signed)?;
        Ok(response(if accepted { Status::Ok } else { Status::Full }))
    }

    fn fetch_keypackage(&self, get: &GetKeyPackage) -> crate::Result<KeyPackageResponse> {
        let (Ok(owner), Ok(mailbox)) = (
            <[u8; 32]>::try_from(get.owner.as_slice()),
            <[u8; 32]>::try_from(get.mailbox.as_slice()),
        ) else {
            return Ok(response(Status::Invalid));
        };
        if get.slot >= u32::from(dyapp_p2p_net::REPLICAS) {
            return Ok(response(Status::Invalid));
        }
        if self.listed(&owner) || self.listed(&mailbox) {
            return Ok(response(Status::Refused));
        }
        let Some((profile, _)) = self.current_device(&owner, &mailbox)? else {
            return Ok(response(Status::NotFound));
        };
        if profile.profile.version < get.profile_version {
            return Ok(response(Status::Stale));
        }
        let now = chrono::Utc::now().timestamp();
        Ok(
            match self
                .store
                .allocate_keypackage(&owner, &mailbox, get.slot as u8, now)?
            {
                Some(record) => KeyPackageResponse {
                    record: Some(record),
                    ..response(Status::Ok)
                },
                None => response(Status::NotFound),
            },
        )
    }
}
