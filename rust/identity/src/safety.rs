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
#[must_use]
pub fn safety_number(mine: &[u8; 32], theirs: &[u8; 32]) -> String {
    from_hashes(&key_hash(mine), &key_hash(theirs))
}

/// The same safety number from canonical lowercase hexadecimal Peer IDs.
/// A verified profile already supplies a Peer ID, so its signer key need not be retained.
#[must_use]
pub fn safety_number_for_peer_ids(mine: &str, theirs: &str) -> Option<String> {
    Some(from_hashes(&parse_peer_id(mine)?, &parse_peer_id(theirs)?))
}

/// The same safety number as it is shown and copied: groups of five digits.
#[must_use]
pub fn grouped_safety_number(mine: &str, theirs: &str) -> Option<String> {
    let digits = safety_number_for_peer_ids(mine, theirs)?;
    let groups = digits
        .as_bytes()
        .chunks(5)
        .map(|group| std::str::from_utf8(group).ok())
        .collect::<Option<Vec<&str>>>()?;
    Some(groups.join(" "))
}

pub(crate) fn parse_peer_id(value: &str) -> Option<[u8; 32]> {
    let bytes = value.as_bytes();
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    let mut peer = [0u8; 32];
    for (index, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
        peer[index] = u8::try_from(char::from(pair[0]).to_digit(16)?).ok()? * 16
            + u8::try_from(char::from(pair[1]).to_digit(16)?).ok()?;
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
#[must_use]
pub fn qr_payload(mine: &[u8; 32], theirs: &[u8; 32]) -> Vec<u8> {
    qr_from_hashes(&key_hash(mine), &key_hash(theirs))
}

/// The same QR payload from canonical lowercase hexadecimal Peer IDs.
#[must_use]
pub fn qr_payload_for_peer_ids(mine: &str, theirs: &str) -> Option<Vec<u8>> {
    Some(qr_from_hashes(
        &parse_peer_id(mine)?,
        &parse_peer_id(theirs)?,
    ))
}

fn qr_from_hashes(mine: &[u8; 32], theirs: &[u8; 32]) -> Vec<u8> {
    [&[VERSION][..], mine, theirs].concat()
}

/// Whether a QR code scanned from the contact's screen names both keys the right way round.
#[must_use]
pub fn qr_matches(scanned: &[u8], mine: &[u8; 32], theirs: &[u8; 32]) -> bool {
    scanned == qr_payload(theirs, mine)
}

/// The same check for canonical lowercase hexadecimal Peer IDs.
#[must_use]
pub fn qr_matches_peer_ids(scanned: &[u8], mine: &str, theirs: &str) -> bool {
    qr_payload_for_peer_ids(theirs, mine).is_some_and(|expected| scanned == expected)
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
        let (ones, twos) = ("11".repeat(32), "22".repeat(32));
        let grouped = grouped_safety_number(&ones, &twos).unwrap();
        assert_eq!(
            grouped.split(' ').map(str::len).collect::<Vec<_>>(),
            [5; 12]
        );
        assert_eq!(
            grouped.replace(' ', ""),
            safety_number_for_peer_ids(&ones, &twos).unwrap()
        );
        assert_eq!(grouped_safety_number(&ones[..63], &twos), None);
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
        let alice_id = crate::peer_id(&alice);
        let bob_id = crate::peer_id(&bob);
        let carol_id = crate::peer_id(&carol);
        assert_eq!(
            qr_payload_for_peer_ids(&bob_id, &alice_id),
            Some(shown_by_bob.clone())
        );
        assert!(qr_matches_peer_ids(&shown_by_bob, &alice_id, &bob_id));
        assert!(!qr_matches_peer_ids(&shown_by_bob, &bob_id, &alice_id));
        assert!(!qr_matches_peer_ids(&shown_by_bob, &alice_id, &carol_id));
        assert!(!qr_matches_peer_ids(
            &shown_by_bob[..64],
            &alice_id,
            &bob_id
        ));
        let mut unknown = shown_by_bob.clone();
        unknown[0] = VERSION + 1;
        assert!(!qr_matches_peer_ids(&unknown, &alice_id, &bob_id));
        assert_eq!(
            qr_payload_for_peer_ids(&bob_id.to_uppercase(), &alice_id),
            None
        );
        assert!(!qr_matches_peer_ids(
            &shown_by_bob,
            &alice_id[..63],
            &bob_id
        ));
    }
}
