//! Safety numbers and QR payloads that two contacts compare out of band.
//!
//! Each side contributes 30 digits from its raw Peer ID, hashed 5200 times like Signal's
//! numeric fingerprint: a man in the middle must find a key whose half matches the victim's, a
//! second preimage of about 100 bits at 5200 hashes per try, not a collision of the whole number.

use crate::key_hash;
use sha2::{Digest, Sha512};

const VERSION: u8 = 1;
const ITERATIONS: usize = 5200;
const LABEL: &[u8] = b"dyapp/safety-number/v1\0";

/// The 60-digit safety number of two identity keys, the same on both sides.
pub fn safety_number(mine: &[u8; 32], theirs: &[u8; 32]) -> String {
    let (mut low, mut high) = (half(mine), half(theirs));
    if low > high {
        std::mem::swap(&mut low, &mut high);
    }
    low + &high
}

// 30 digits: six 5-byte chunks of the iterated hash, each mod 100000.
fn half(public_key: &[u8; 32]) -> String {
    let peer = key_hash(public_key);
    let mut hash: [u8; 64] = Sha512::new()
        .chain_update(LABEL)
        .chain_update(peer)
        .finalize()
        .into();
    for _ in 1..ITERATIONS {
        hash = Sha512::new()
            .chain_update(hash)
            .chain_update(peer)
            .finalize()
            .into();
    }
    hash[..30]
        .chunks(5)
        .map(|chunk| {
            let value = chunk
                .iter()
                .fold(0u64, |value, byte| value << 8 | u64::from(*byte));
            format!("{:05}", value % 100_000)
        })
        .collect()
}

/// What this profile's QR code carries: a version, then its raw Peer ID and the contact's.
pub fn qr_payload(mine: &[u8; 32], theirs: &[u8; 32]) -> Vec<u8> {
    [&[VERSION][..], &key_hash(mine), &key_hash(theirs)].concat()
}

/// Whether a QR code scanned from the contact's screen names both keys the right way round.
pub fn qr_matches(scanned: &[u8], mine: &[u8; 32], theirs: &[u8; 32]) -> bool {
    scanned == qr_payload(theirs, mine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_see_one_number_unique_to_the_pair() {
        let (alice, bob, carol) = ([1; 32], [2; 32], [3; 32]);
        let number = safety_number(&alice, &bob);
        assert_eq!(number.len(), 60);
        assert!(number.bytes().all(|byte| byte.is_ascii_digit()));
        assert_eq!(number, safety_number(&bob, &alice));
        assert_ne!(number, safety_number(&alice, &carol));
        let (a, b) = (half(&alice), half(&bob));
        assert!(number == a.clone() + &b || number == b + &a);
    }

    #[test]
    fn a_scan_matches_only_the_pair_in_order() {
        let (alice, bob, carol) = ([1; 32], [2; 32], [3; 32]);
        let shown_by_bob = qr_payload(&bob, &alice);
        assert_eq!(shown_by_bob.len(), 65);
        assert!(qr_matches(&shown_by_bob, &alice, &bob));
        assert!(!qr_matches(&qr_payload(&alice, &bob), &alice, &bob));
        assert!(!qr_matches(&qr_payload(&carol, &alice), &alice, &bob));
        assert!(!qr_matches(&shown_by_bob[..64], &alice, &bob));
    }
}
