//! The hash puzzle a loaded search node asks of client queries and gets: SHA-256 of
//! "dyapp-search-puzzle", the challenge, the base58 Peer ID and the big-endian nonce must start
//! with `bits` zero bits.

/// Whether `nonce` solves `challenge` for `peer` at `bits`.
pub fn solved(challenge: &[u8], peer: &str, nonce: u64, bits: u32) -> bool {
    let mut data = b"dyapp-search-puzzle".to_vec();
    data.extend_from_slice(challenge);
    data.extend_from_slice(peer.as_bytes());
    data.extend_from_slice(&nonce.to_be_bytes());
    let hash = dyapp_identity::sha256(&data);
    let zeros = u128::from_be_bytes(hash[..16].try_into().unwrap_or_default()).leading_zeros();
    zeros >= bits
}

/// A nonce solving `challenge` for `peer` at `bits`.
pub fn solve(challenge: &[u8; 16], peer: &str, bits: u32) -> u64 {
    solve_from(challenge, peer, bits, 0)
}

/// The first nonce from `start` solving `challenge` for `peer` at `bits`.
pub fn solve_from(challenge: &[u8; 16], peer: &str, bits: u32, start: u64) -> u64 {
    let mut nonce = start;
    while !solved(challenge, peer, nonce, bits) {
        nonce += 1;
    }
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solution_is_the_first_nonce_and_bound_to_its_inputs() {
        let challenge = [3; 16];
        assert_eq!(solve(&challenge, "peer", 0), 0);
        let nonce = solve(&challenge, "peer", 10);
        assert!(solved(&challenge, "peer", nonce, 10));
        assert!((0..nonce).all(|n| !solved(&challenge, "peer", n, 10)));
        assert!(solve_from(&challenge, "peer", 10, nonce + 1) > nonce);
        assert!(!solved(&challenge, "other", nonce, 10) || !solved(&[4; 16], "peer", nonce, 10));
        // Leading zeros count across the whole first 128 bits.
        assert!(!solved(&challenge, "peer", nonce, 128));
    }
}
