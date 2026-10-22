#![no_main]
//! MLS input a client takes from the network: KeyPackages, Welcomes and group messages.

use dyapp_fuzz::split;
use dyapp_identity::Identity;
use dyapp_messaging::mls::{group_of, Device, Group};
use dyapp_profile::{Profile, VerifiedProfile};
use std::sync::{LazyLock, Mutex};

struct Mls {
    alice: Device,
    bob: Device,
    group: Group,
    profiles: Vec<VerifiedProfile>,
}

/// Alice's side of a group with Bob, and the profiles listing both devices.
static MLS: LazyLock<Mutex<Mls>> = LazyLock::new(|| {
    let member = |secret| {
        let identity = Identity::from_secret(&[secret; 32]);
        let device = Device::new(&identity, [secret; 32], 1).unwrap();
        let profile = VerifiedProfile {
            peer_id: identity.peer_id(),
            profile: Profile {
                devices: vec![device.certificate().clone()],
                ..Profile::default()
            },
        };
        (device, profile)
    };
    let (alice, alice_profile) = member(1);
    let (bob, bob_profile) = member(2);
    let profiles = vec![alice_profile, bob_profile];
    let (group, _) = alice
        .create_group(&[bob.key_package(false).unwrap()], &profiles)
        .unwrap();
    Mutex::new(Mls {
        alice,
        bob,
        group,
        profiles,
    })
});

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let mut mls = MLS.lock().unwrap();
    let Mls {
        alice,
        bob,
        group,
        profiles,
    } = &mut *mls;
    match selector % 4 {
        0 => drop(group_of(data)),
        1 => drop(alice.create_group(&[data.to_vec()], profiles)),
        2 => drop(bob.join(data, profiles)),
        _ => drop(group.process(alice, data, profiles)),
    }
});
