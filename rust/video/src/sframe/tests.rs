use super::*;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

const BASE_KEY: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

// Alice is member 0 and Bob member 1 of epoch 3; Bob receives from Alice.
fn call() -> (Sender, Receiver) {
    let mut bob = Receiver::default();
    bob.add_epoch(&BASE_KEY, 3, &[0]).unwrap();
    (Sender::new(&BASE_KEY, 3, 0).unwrap(), bob)
}

#[test]
fn header_matches_rfc_vectors() {
    for (kid, ctr, encoded) in [
        (0, 0, "00"),
        (0, 1, "01"),
        (0, 0xff, "08ff"),
        (0, 0x100, "090100"),
        (u64::MAX, 0, "f0ffffffffffffffff"),
        (u64::MAX, 0x100, "f9ffffffffffffffff0100"),
        (0x123, 0x4567, "9901234567"),
        (7, u64::MAX, "7fffffffffffffffff"),
    ] {
        assert_eq!(header(kid, ctr), hex(encoded), "{kid:x} {ctr:x}");
        assert_eq!(parse(&hex(encoded)), Some((kid, ctr, encoded.len() / 2)));
    }
    assert_eq!(parse(&[]), None);
    assert_eq!(parse(&hex("f0ffff")), None);
}

#[test]
fn encryption_matches_the_rfc_vector() {
    let kid = 0x123;
    let mut sender = Sender {
        key: Key::derive(&BASE_KEY, kid).unwrap(),
        kid,
        ctr: 0x4567,
    };
    assert_eq!(sender.key.salt.to_vec(), hex("75234edefe07819026751816"));
    assert_eq!(
        sender.key.nonce(0x4567).to_vec(),
        hex("75234edefe07819026755d71")
    );
    let metadata = hex("4945544620534672616d65205747");
    let ciphertext = sender.encrypt(&metadata, b"draft-ietf-sframe-enc").unwrap();
    let expected =
        "9901234567b7412c2513a1b66dbb48841bbaf17f598751176ad847681a69c6d0b091c07018ce4adb34eb";
    assert_eq!(ciphertext, hex(expected));
}

#[test]
fn frames_round_trip_and_every_counter_is_new() {
    let (mut alice, mut bob) = call();
    let mut nonces = std::collections::HashSet::new();
    for frame in 0..300u32 {
        let ciphertext = alice.encrypt(b"meta", &frame.to_be_bytes()).unwrap();
        let (_, ctr, _) = parse(&ciphertext).unwrap();
        assert!(
            nonces.insert(alice.key.nonce(ctr)),
            "nonce reused at {frame}"
        );
        assert_eq!(
            bob.decrypt(b"meta", &ciphertext).unwrap(),
            frame.to_be_bytes()
        );
    }
    alice.ctr = u64::MAX;
    assert!(
        alice.encrypt(b"", b"last").is_err(),
        "the counter never wraps"
    );
}

#[test]
fn tampered_wrong_key_and_wrong_metadata_fail() {
    let (mut alice, mut bob) = call();
    let ciphertext = alice.encrypt(b"meta", b"frame").unwrap();
    for at in 0..ciphertext.len() {
        let mut tampered = ciphertext.clone();
        tampered[at] ^= 1;
        assert!(bob.decrypt(b"meta", &tampered).is_err(), "byte {at}");
    }
    assert!(bob.decrypt(b"other", &ciphertext).is_err());
    let mut stranger = Receiver::default();
    stranger.add_epoch(&[9; 16], 3, &[0]).unwrap();
    assert!(stranger.decrypt(b"meta", &ciphertext).is_err());
    assert_eq!(bob.decrypt(b"meta", &ciphertext).unwrap(), b"frame");
}

#[test]
fn replayed_and_too_old_frames_are_dropped() {
    let (mut alice, mut bob) = call();
    let frames: Vec<_> = (0..100)
        .map(|_| alice.encrypt(b"", b"f").unwrap())
        .collect();
    bob.decrypt(b"", &frames[50]).unwrap();
    assert!(bob.decrypt(b"", &frames[50]).is_err(), "replay");
    bob.decrypt(b"", &frames[10]).unwrap();
    assert!(
        bob.decrypt(b"", &frames[10]).is_err(),
        "replay below the top"
    );
    bob.decrypt(b"", &frames[99]).unwrap();
    assert!(
        bob.decrypt(b"", &frames[35]).is_err(),
        "older than the window"
    );
    bob.decrypt(b"", &frames[36]).unwrap();
    assert!(
        bob.decrypt(b"", &frames[50]).is_err(),
        "kept across a shift"
    );
}

#[test]
fn senders_have_their_own_keys_and_new_epochs_retire_old_ones() {
    let mut bob = Receiver::default();
    bob.add_epoch(&BASE_KEY, 3, &[0, 2]).unwrap();
    let mut alice = Sender::new(&BASE_KEY, 3, 0).unwrap();
    let mut carol = Sender::new(&BASE_KEY, 3, 2).unwrap();
    let from_alice = alice.encrypt(b"", b"same").unwrap();
    let from_carol = carol.encrypt(b"", b"same").unwrap();
    assert_ne!(from_alice[1..], from_carol[1..]);
    assert_eq!(bob.decrypt(b"", &from_carol).unwrap(), b"same");

    // Epoch 19 has the same low-order bits as epoch 3, so it replaces it.
    bob.add_epoch(&[7; 16], 19, &[0]).unwrap();
    assert!(bob.decrypt(b"", &from_alice).is_err());
    let mut next = Sender::new(&[7; 16], 19, 0).unwrap();
    assert_eq!(
        bob.decrypt(b"", &next.encrypt(b"", b"new").unwrap())
            .unwrap(),
        b"new"
    );
    assert_eq!(kid(19, 2), 0x23);
}
