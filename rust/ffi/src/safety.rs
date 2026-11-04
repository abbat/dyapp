//! The numbers two contacts compare out of band, so every client shows the same ones.

/// The 60 digits of a pair in groups of five; none for a Peer ID that is not canonical.
#[uniffi::export]
pub fn safety_number_text(mine: String, theirs: String) -> Option<String> {
    dyapp_identity::safety::grouped_safety_number(&mine, &theirs)
}

/// The bytes of the QR code `mine` shows to `theirs`; none for a Peer ID that is not canonical.
#[uniffi::export]
pub fn safety_qr_payload(mine: String, theirs: String) -> Option<Vec<u8>> {
    dyapp_identity::safety::qr_payload_for_peer_ids(&mine, &theirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_gets_the_shared_number_and_an_ordered_code() {
        let (a, b) = ("00".repeat(32), "11".repeat(32));
        let number = safety_number_text(a.clone(), b.clone()).unwrap();
        assert_eq!(
            number.replace(' ', ""),
            dyapp_identity::safety::safety_number_for_peer_ids(&a, &b).unwrap()
        );
        assert_eq!(safety_number_text(b.clone(), a.clone()).unwrap(), number);
        let shown = safety_qr_payload(a.clone(), b.clone()).unwrap();
        assert!(dyapp_identity::safety::qr_matches_peer_ids(&shown, &b, &a));
        assert_ne!(safety_qr_payload(b.clone(), a.clone()).unwrap(), shown);
    }

    #[test]
    fn a_peer_id_that_is_not_canonical_gets_nothing() {
        let (good, bad) = ("00".repeat(32), "AA".repeat(32));
        assert_eq!(safety_number_text(good.clone(), bad.clone()), None);
        assert_eq!(safety_qr_payload(bad, good), None);
    }
}
