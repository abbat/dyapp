//! What a node does with a peer's identify info: drops a peer of another network, routes to and
//! remembers a node of its own.

use super::*;

/// The identify version of a peer of another network is logged up to this many bytes.
const MAX_LOGGED_VERSION: usize = 80;

/// Disconnects a peer of another network (ADR 0017): identify and AutoNAT are the only protocols
/// networks share. `dialed` is the address when this node dialled it, a seed or anchor of the
/// wrong network.
pub(super) fn drop_foreign(
    swarm: &mut Swarm<Behaviour>,
    dialed: Option<&Multiaddr>,
    peer_id: PeerId,
    version: &str,
) {
    let version = cut(version, MAX_LOGGED_VERSION);
    match dialed {
        Some(address) => tracing::warn!(
            %peer_id,
            %address,
            ?version,
            "dialled a node of another network: check seeds and anchors"
        ),
        None => tracing::debug!(%peer_id, ?version, "peer of another network dropped"),
    }
    let _ = swarm.disconnect_peer_id(peer_id);
}

/// Adds a node of this network to the search DHT when it serves search and to the node DHT, and
/// remembers it as an anchor when this node dialled it. Whether it serves the node DHT.
// ponytail: claimed addresses pass the IP-group limits but are not verified by a dial; a peer
// can claim another group's address.
pub(super) fn add_node(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    anchors: &mut Vec<Multiaddr>,
    dialed: Option<&Multiaddr>,
    peer_id: PeerId,
    info: identify::Info,
) -> bool {
    let bits = dyapp_p2p_net::id_pow_bits();
    // Only search nodes serve the search DHT, so its protocol marks one.
    if info.protocols.contains(&service.protocols.search_kad)
        && dyapp_p2p_net::id_has_pow(&peer_id, bits)
    {
        for address in &info.listen_addrs {
            dyapp_p2p_net::add_search_peer(swarm, peer_id, address.clone());
        }
        service.search_peer(&peer_id.to_string(), true);
    }
    if !info.protocols.contains(&service.protocols.kad) {
        return false;
    }
    // A routable peer this node dialled answered: it becomes an anchor.
    if let Some(address) = dialed {
        if dyapp_p2p_net::id_has_pow(&peer_id, bits) {
            remember_anchor(anchors, peer_id, address);
        }
    }
    // Peers enter the routing table here, not through kad's own inserts.
    for address in info.listen_addrs {
        dyapp_p2p_net::add_peer(swarm, peer_id, address);
    }
    true
}

/// `s` cut to at most `max` bytes at a character boundary.
fn cut(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::cut;

    #[test]
    fn cut_keeps_whole_characters() {
        assert_eq!(cut("ab", 80), "ab");
        assert_eq!(cut("aй", 2), "a");
        assert_eq!(cut("aй", 3), "aй");
    }
}
