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

/// Outcome of a directory read; absence of a verified profile grants no revocation authority.
#[derive(Clone, Copy)]
pub enum Readback<'a> {
    /// No network read was requested.
    Cached,
    /// The requested read could not complete.
    Unavailable,
    /// The read returned no verified same-owner profile.
    NoProfile,
    /// A verified same-owner profile was returned.
    Verified(&'a Profile),
}

/// Whether the requested directory read completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadState {
    /// Saved data only.
    Cached,
    /// Network or configuration was unavailable.
    Unavailable,
    /// No verified same-owner profile was returned.
    NoProfile,
    /// Verified same-owner readback completed, possibly older than the saved draft.
    Verified,
}

/// Snapshot actually used for the displayed rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowSource {
    /// Saved profile, including fallback after an unavailable or older readback.
    Saved,
    /// Strictly newer verified profile.
    Remote,
}

/// Truthful read status and rows; this snapshot authorizes no device changes.
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Current-device-first public selectors.
    pub devices: Vec<OwnDevice>,
    /// Read outcome, independent of whether rows are empty.
    pub state: ReadState,
    /// Profile actually projected into rows.
    pub source: RowSource,
}

/// Project read status and directory contents without guessing freshness from row contents.
#[must_use]
pub fn snapshot(draft: &Profile, readback: Readback<'_>, this: &[u8]) -> Snapshot {
    let (state, remote) = match readback {
        Readback::Cached => (ReadState::Cached, None),
        Readback::Unavailable => (ReadState::Unavailable, None),
        Readback::NoProfile => (ReadState::NoProfile, None),
        Readback::Verified(profile) => (ReadState::Verified, Some(profile)),
    };
    let source = if newer(draft, remote).is_some() {
        RowSource::Remote
    } else {
        RowSource::Saved
    };
    Snapshot {
        devices: rows(draft, remote, this),
        state,
        source,
    }
}

/// Project current-device-first rows, preserving other order and the saved editable profile.
///
/// `remote` must come from verified same-owner ordinary readback. An unavailable readback is
/// `None`; equal or older remote versions do not replace the saved draft's directory. This
/// preserves the desktop's existing offline fallback and strict version precedence.
#[must_use]
pub fn rows(draft: &Profile, remote: Option<&Profile>, this: &[u8]) -> Vec<OwnDevice> {
    let profile = newer(draft, remote).unwrap_or(draft);
    let mut rows: Vec<_> = publish::device_keys(profile)
        .into_iter()
        .map(|key| OwnDevice {
            this: key == this,
            key,
        })
        .collect();
    rows.sort_by_key(|row| !row.this);
    rows
}

fn newer<'a>(draft: &Profile, remote: Option<&'a Profile>) -> Option<&'a Profile> {
    remote.filter(|held| held.version > draft.version)
}

#[cfg(test)]
#[path = "device_directory/read_tests.rs"]
mod read_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::{Domain, Identity};
    use dyapp_profile::DeviceCertificate;
    use prost::Message as _;

    pub(super) fn profile(owner: &Identity, device: &Identity, version: u64) -> Profile {
        Profile {
            version,
            name: "unsent local edit".into(),
            devices: vec![owner.sign(
                Domain::DeviceCertificate,
                DeviceCertificate {
                    device_key: device.public_key().to_vec(),
                    mls_signature_key: Identity::try_generate().unwrap().public_key().to_vec(),
                    generation: 1,
                }
                .encode_to_vec(),
            )],
            ..Default::default()
        }
    }

    #[test]
    fn offline_equal_and_older_readbacks_preserve_the_local_directory_and_fields() {
        let owner = Identity::try_generate().unwrap();
        let local_device = Identity::try_generate().unwrap();
        let other = Identity::try_generate().unwrap();
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
        let owner = Identity::try_generate().unwrap();
        let local_device = Identity::try_generate().unwrap();
        let other = Identity::try_generate().unwrap();
        let local = profile(&owner, &local_device, 5);
        let mut remote = profile(&owner, &other, 6);
        remote.devices.extend(local.devices.clone());
        let before = (local.clone(), remote.clone());
        assert_eq!(
            rows(&local, Some(&remote), &local_device.public_key()),
            vec![
                OwnDevice {
                    key: local_device.public_key().to_vec(),
                    this: true
                },
                OwnDevice {
                    key: other.public_key().to_vec(),
                    this: false
                },
            ]
        );
        assert_eq!((local, remote), before);
    }

    #[test]
    fn current_first_preserves_other_devices_order_and_absent_current_keeps_all_rows() {
        let owner = Identity::try_generate().unwrap();
        let current = Identity::try_generate().unwrap();
        let first = Identity::try_generate().unwrap();
        let second = Identity::try_generate().unwrap();
        let draft = profile(&owner, &current, 5);
        let others = [profile(&owner, &first, 6), profile(&owner, &second, 6)];
        let mut remote = others[0].clone();
        remote.devices.extend(others[1].devices.clone());
        let absent = vec![
            OwnDevice {
                key: first.public_key().to_vec(),
                this: false,
            },
            OwnDevice {
                key: second.public_key().to_vec(),
                this: false,
            },
        ];
        assert_eq!(rows(&draft, Some(&remote), &current.public_key()), absent);
        for position in 0..=remote.devices.len() {
            let mut held = remote.clone();
            held.devices.insert(position, draft.devices[0].clone());
            let before = (draft.clone(), held.clone());
            let mut expected = vec![OwnDevice {
                key: current.public_key().to_vec(),
                this: true,
            }];
            expected.extend(absent.clone());
            assert_eq!(rows(&draft, Some(&held), &current.public_key()), expected);
            assert_eq!((draft.clone(), held), before);
        }
    }

    #[test]
    fn newer_removed_profile_has_no_devices_instead_of_restoring_cached_authorization() {
        let owner = Identity::try_generate().unwrap();
        let device = Identity::try_generate().unwrap();
        let local = profile(&owner, &device, 5);
        let removed = Profile {
            version: 6,
            deleted: true,
            ..Default::default()
        };
        assert_eq!(rows(&local, Some(&removed), &device.public_key()), vec![]);
    }
}
