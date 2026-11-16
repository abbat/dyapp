//! Shared presentation of the owner's saved or verified remote device directory.
use crate::publish;
use dyapp_profile::Profile;

/// Public mailbox key and whether it belongs to the current device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnDevice {
    /// Public device key, never an identity or MLS secret.
    pub key: Vec<u8>,
    /// This client's existing device key matches the listed key.
    pub this: bool,
}

/// Project the directory without changing the saved editable profile.
///
/// `remote` must come from verified same-owner ordinary readback. An unavailable readback is
/// `None`; equal or older remote versions do not replace the saved draft's directory. This
/// preserves the desktop's existing offline fallback and strict version precedence.
#[must_use]
pub fn rows(draft: &Profile, remote: Option<&Profile>, this: &[u8]) -> Vec<OwnDevice> {
    let profile = remote
        .filter(|held| held.version > draft.version)
        .unwrap_or(draft);
    publish::device_keys(profile)
        .into_iter()
        .map(|key| OwnDevice {
            this: key == this,
            key,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::{Domain, Identity};
    use dyapp_profile::DeviceCertificate;
    use prost::Message as _;

    fn profile(owner: &Identity, device: &Identity, version: u64) -> Profile {
        Profile {
            version,
            name: "unsent local edit".into(),
            devices: vec![owner.sign(
                Domain::DeviceCertificate,
                DeviceCertificate {
                    device_key: device.public_key().to_vec(),
                    mls_signature_key: Identity::generate().public_key().to_vec(),
                    generation: 1,
                }
                .encode_to_vec(),
            )],
            ..Default::default()
        }
    }

    #[test]
    fn offline_equal_and_older_readbacks_preserve_the_local_directory_and_fields() {
        let owner = Identity::generate();
        let local_device = Identity::generate();
        let other = Identity::generate();
        let local = profile(&owner, &local_device, 5);
        let before = local.clone();
        let expected = vec![OwnDevice {
            key: local_device.public_key().to_vec(),
            this: true,
        }];
        assert_eq!(rows(&local, None, &local_device.public_key()), expected);
        for version in [4, 5] {
            let remote = profile(&owner, &other, version);
            assert_eq!(
                rows(&local, Some(&remote), &local_device.public_key()),
                expected
            );
        }
        assert_eq!(local, before);
    }

    #[test]
    fn newer_verified_directory_marks_each_actual_device_without_mutating_either_profile() {
        let owner = Identity::generate();
        let local_device = Identity::generate();
        let other = Identity::generate();
        let local = profile(&owner, &local_device, 5);
        let mut remote = profile(&owner, &other, 6);
        remote.devices.extend(local.devices.clone());
        let before = (local.clone(), remote.clone());
        assert_eq!(
            rows(&local, Some(&remote), &local_device.public_key()),
            vec![
                OwnDevice {
                    key: other.public_key().to_vec(),
                    this: false
                },
                OwnDevice {
                    key: local_device.public_key().to_vec(),
                    this: true
                },
            ]
        );
        assert_eq!((local, remote), before);
    }

    #[test]
    fn newer_removed_profile_has_no_devices_instead_of_restoring_cached_authorization() {
        let owner = Identity::generate();
        let device = Identity::generate();
        let local = profile(&owner, &device, 5);
        let removed = Profile {
            version: 6,
            deleted: true,
            ..Default::default()
        };
        assert_eq!(rows(&local, Some(&removed), &device.public_key()), vec![]);
    }
}
