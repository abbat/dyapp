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
    from_hashes(&key_hash(mine), &key_hash(theirs))
}

/// The same safety number from canonical lowercase hexadecimal Peer IDs.
/// A verified profile already supplies a Peer ID, so its signer key need not be retained.
pub fn safety_number_for_peer_ids(mine: &str, theirs: &str) -> Option<String> {
    Some(from_hashes(&parse_peer_id(mine)?, &parse_peer_id(theirs)?))
}

fn parse_peer_id(value: &str) -> Option<[u8; 32]> {
    let bytes = value.as_bytes();
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    let mut peer = [0u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        peer[index] =
            (pair[0] as char).to_digit(16)? as u8 * 16 + (pair[1] as char).to_digit(16)? as u8;
    }
    Some(peer)
}

fn from_hashes(mine: &[u8; 32], theirs: &[u8; 32]) -> String {
    let (mut low, mut high) = (half(mine), half(theirs));
    if low > high {
        std::mem::swap(&mut low, &mut high);
    }
    low + &high
}

// 30 digits: six 5-byte chunks of the iterated hash, each mod 100000.
fn half(peer: &[u8; 32]) -> String {
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
        let (a, b) = (half(&key_hash(&alice)), half(&key_hash(&bob)));
        assert!(number == a.clone() + &b || number == b + &a);
        let alice_id = crate::peer_id(&alice);
        let bob_id = crate::peer_id(&bob);
        assert_eq!(
            safety_number_for_peer_ids(&alice_id, &bob_id),
            Some(number.clone())
        );
        assert_eq!(safety_number_for_peer_ids(&bob_id, &alice_id), Some(number));
        assert_eq!(
            safety_number_for_peer_ids(&alice_id.to_uppercase(), &bob_id),
            None
        );
        assert_eq!(safety_number_for_peer_ids(&alice_id[..63], &bob_id), None);
        assert_eq!(
            safety_number_for_peer_ids(&format!("g{}", &alice_id[1..]), &bob_id),
            None
        );
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
