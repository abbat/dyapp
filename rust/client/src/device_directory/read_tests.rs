//! Equal row contents must not erase distinct read outcomes or version precedence.
use super::tests::profile;
use super::*;
use dyapp_identity::Identity;

#[test]
fn identical_saved_rows_distinguish_cache_failure_and_no_verified_profile() {
    let owner = Identity::from_secret(&[7; 32]);
    let device = Identity::from_secret(&[8; 32]);
    let draft = profile(&owner, &device, 5);
    let before = draft.clone();
    for (readback, state) in [
        (Readback::Cached, ReadState::Cached),
        (Readback::Unavailable, ReadState::Unavailable),
        (Readback::NoProfile, ReadState::NoProfile),
    ] {
        assert_eq!(
            snapshot(&draft, readback, &device.public_key()),
            Snapshot {
                devices: vec![OwnDevice {
                    key: device.public_key().to_vec(),
                    this: true
                }],
                state,
                source: RowSource::Saved,
            }
        );
    }
    assert_eq!(draft, before);
}

#[test]
fn verified_readback_reports_the_actual_row_source_without_replacing_a_newer_draft() {
    let owner = Identity::from_secret(&[7; 32]);
    let device = Identity::from_secret(&[8; 32]);
    let other = Identity::from_secret(&[9; 32]);
    let draft = profile(&owner, &device, 5);
    for (version, source, key, current) in [
        (4, RowSource::Saved, device.public_key(), true),
        (5, RowSource::Saved, device.public_key(), true),
        (6, RowSource::Remote, other.public_key(), false),
    ] {
        let remote = profile(&owner, &other, version);
        let before = (draft.clone(), remote.clone());
        assert_eq!(
            snapshot(&draft, Readback::Verified(&remote), &device.public_key()),
            Snapshot {
                devices: vec![OwnDevice {
                    key: key.to_vec(),
                    this: current
                }],
                state: ReadState::Verified,
                source,
            }
        );
        assert_eq!((draft.clone(), remote), before);
    }
}

#[test]
fn verified_removed_directory_is_distinct_from_an_empty_unavailable_cache() {
    let owner = Identity::from_secret(&[7; 32]);
    let device = Identity::from_secret(&[8; 32]);
    let draft = profile(&owner, &device, 5);
    let removed = Profile {
        version: 6,
        deleted: true,
        ..Profile::default()
    };
    assert_eq!(
        snapshot(&draft, Readback::Verified(&removed), &device.public_key()),
        Snapshot {
            devices: vec![],
            state: ReadState::Verified,
            source: RowSource::Remote,
        }
    );
    assert_eq!(
        snapshot(
            &Profile::default(),
            Readback::Unavailable,
            &device.public_key()
        ),
        Snapshot {
            devices: vec![],
            state: ReadState::Unavailable,
            source: RowSource::Saved,
        }
    );
}
