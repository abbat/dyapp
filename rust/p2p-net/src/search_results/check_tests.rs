use super::*;
use dyapp_identity::Identity;
use std::cell::Cell;

#[test]
fn pow_runs_once_per_owner_across_replies() {
    let [a, b] = [(); 2].map(|()| Identity::try_generate().unwrap());
    let record = |identity: &Identity, version| {
        Profile {
            version,
            pow_bits: 1,
            ..Profile::default()
        }
        .sign(identity)
    };
    let replies = [
        vec![record(&a, 1), record(&b, 1)],
        vec![record(&a, 2), record(&b, 1)],
    ];
    let calls = Cell::new(0);
    let found = check_with(&[], 0, &replies, |_, _| {
        calls.set(calls.get() + 1);
        1
    });
    assert_eq!(calls.get(), 2);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].profile.version, 2);
}

#[test]
fn pow_is_checked() {
    let owner = Identity::try_generate().unwrap();
    let lying = Profile {
        version: 1,
        pow_bits: 30,
        ..Profile::default()
    }
    .sign(&owner);
    assert_eq!(check(&[], 0, &[vec![lying]]), [] as [VerifiedProfile; 0]);
    let key: [u8; 32] = owner.public_key();
    let nonce = dyapp_profile::solve(&key, 2, 0, 1000).unwrap();
    let mined = Profile {
        version: 1,
        pow_bits: 2,
        pow_nonce: nonce,
        ..Profile::default()
    }
    .sign(&owner);
    assert_eq!(check(&[], 2, &[vec![mined]]).len(), 1);
}
