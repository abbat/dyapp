//! What a node does with a peer's identify info: drops a peer of another network, routes to and
//! remembers a node of its own.

use super::*;
use crate::claims::Claims;
use dyapp_p2p_net::Endpoints;
use libp2p::core::ConnectedPoint;
use libp2p::kad::store::RecordStore as _;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};

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

/// Which addresses of a peer enter the DHTs: an identify listen address on the IP of the
/// connection it came over at once, any other after a dial with the peer's ID, to only that
/// address, reaches that peer. The connection's own remote address is the evidence, never the
/// address the peer says it sees for this node.
#[derive(Default)]
pub(super) struct Admission {
    /// The peer and remote address of each open connection, and whether this node dialled it.
    remotes: HashMap<ConnectionId, (PeerId, Multiaddr, bool)>,
    claims: Claims,
    /// Whether each connected peer with claims routes in the node DHT and the search DHT.
    roles: HashMap<PeerId, (bool, bool)>,
}

impl Admission {
    /// A connection was established; when it is the dial of a claim by the same peer, the
    /// claimed address enters the DHTs the peer serves.
    pub(super) fn established(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        id: ConnectionId,
        peer: PeerId,
        endpoint: &ConnectedPoint,
    ) {
        let remote = endpoint.get_remote_address().clone();
        self.remotes
            .insert(id, (peer, remote, endpoint.is_dialer()));
        if let Some(address) = self.claims.confirm(id, peer) {
            let (kad, search) = self.roles.get(&peer).copied().unwrap_or_default();
            if search {
                dyapp_p2p_net::add_search_peer(swarm, peer, address.clone());
            }
            if kad {
                dyapp_p2p_net::add_peer(swarm, peer, address);
            }
        }
        self.start(swarm);
    }

    /// A connection closed; the peer's queued claims go with its last one.
    pub(super) fn closed(&mut self, id: ConnectionId, peer: PeerId, last: bool) {
        self.remotes.remove(&id);
        if last {
            self.claims.disconnected(&peer);
            self.roles.remove(&peer);
        }
    }

    /// An outgoing connection failed: a claim dialled on it is dropped.
    pub(super) fn failed(&mut self, swarm: &mut Swarm<Behaviour>, id: ConnectionId) {
        self.claims.fail(id);
        self.start(swarm);
    }

    /// The remote address of a connection this node dialled.
    pub(super) fn dialed(&self, id: &ConnectionId) -> Option<&Multiaddr> {
        self.remotes
            .get(id)
            .filter(|(_, _, dialer)| *dialer)
            .map(|(_, a, _)| a)
    }

    /// Accept a search or TURN provider whose peer ID carries the proof of work of `bits` and
    /// whose every address has a live connection of that peer on the same exact IP. The sender
    /// may be a different peer forwarding the record.
    pub(super) fn provider_allowed(
        &self,
        endpoints: &Endpoints,
        record: &kad::ProviderRecord,
        bits: u32,
    ) -> bool {
        [dyapp_p2p_net::SEARCH_KEY, dyapp_p2p_net::TURN_KEY].contains(&record.key.as_ref())
            && dyapp_p2p_net::id_has_pow(&record.provider, bits)
            && !record.addresses.is_empty()
            && record.addresses.iter().all(|address| {
                self.remotes.iter().any(|(id, (peer, _, _))| {
                    *peer == record.provider
                        && dyapp_p2p_net::same_ip(&self.remote(endpoints, *id), address)
                })
            })
    }

    /// The connection's remote address: its resolved IP once known, so a DNS dial proves the
    /// IP it reached, else the address it was established with.
    fn remote(&self, endpoints: &Endpoints, id: ConnectionId) -> Multiaddr {
        match (endpoints.ip(id), self.remotes.get(&id)) {
            (Some(ip), _) => Multiaddr::from(ip),
            (None, Some((_, remote, _))) => remote.clone(),
            (None, None) => Multiaddr::empty(),
        }
    }

    /// Store only admissible provider records. FilterBoth leaves all other remote records out.
    pub(super) fn add_provider(&self, swarm: &mut Swarm<Behaviour>, record: kad::ProviderRecord) {
        let bits = dyapp_p2p_net::id_pow_bits();
        if self.provider_allowed(&swarm.behaviour().endpoints, &record, bits) {
            if let Err(error) = swarm.behaviour_mut().kad.store_mut().add_provider(record) {
                tracing::debug!(%error, "DHT provider record not stored");
            }
        }
    }

    /// Open connections.
    pub(super) fn len(&self) -> usize {
        self.remotes.len()
    }

    fn start(&mut self, swarm: &mut Swarm<Behaviour>) {
        self.claims.start(Instant::now(), |peer, address| {
            let dial = DialOpts::peer_id(peer)
                .condition(PeerCondition::Always)
                .addresses(vec![address.clone()])
                .build();
            let id = dial.connection_id();
            swarm.dial(dial).ok().map(|()| id)
        });
    }
}

/// Adds a node of this network to the search DHT when it serves search and to the node DHT, and
/// remembers it as an anchor when this node dialled it. Listen addresses go through
/// [`Admission`]. Whether it serves the node DHT.
pub(super) fn add_node(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    anchors: &mut Vec<Multiaddr>,
    admission: &mut Admission,
    id: ConnectionId,
    peer_id: PeerId,
    info: identify::Info,
) -> bool {
    let pow = dyapp_p2p_net::id_has_pow(&peer_id, dyapp_p2p_net::id_pow_bits());
    // Only search nodes serve the search DHT, so its protocol marks one.
    let search = pow && info.protocols.contains(&service.protocols.search_kad);
    let kad = info.protocols.contains(&service.protocols.kad);
    if !admission.remotes.contains_key(&id) {
        return false;
    }
    let remote = admission.remote(&swarm.behaviour().endpoints, id);
    let mut proved = Vec::new();
    if search || (kad && pow) {
        admission.roles.insert(peer_id, (kad, search));
        let now = Instant::now();
        for address in info.listen_addrs {
            if dyapp_p2p_net::same_ip(&remote, &address) {
                proved.push(address);
            } else if !admission.claims.claim(now, peer_id, address) {
                tracing::debug!(%peer_id, "claimed address not queued for a probe");
            }
        }
        admission.start(swarm);
    }
    if search {
        for address in &proved {
            dyapp_p2p_net::add_search_peer(swarm, peer_id, address.clone());
        }
        service.search_peer(&peer_id.to_string(), true);
    }
    if !kad {
        return false;
    }
    // A routable peer this node dialled answered: it becomes an anchor.
    if let Some(address) = admission.dialed(&id) {
        if pow {
            remember_anchor(anchors, peer_id, address);
        }
    }
    // Peers enter the routing table here, not through kad's own inserts.
    for address in proved {
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
    use super::*;
    use libp2p::core::transport::PortUse;
    use libp2p::core::Endpoint;

    #[test]
    fn cut_keeps_whole_characters() {
        assert_eq!(cut("ab", 80), "ab");
        assert_eq!(cut("aй", 2), "a");
        assert_eq!(cut("aй", 3), "aй");
    }

    #[test]
    fn provider_addresses_need_the_advertised_peers_live_ip() {
        let provider = PeerId::random();
        let sender = PeerId::random();
        let provider_connection = ConnectionId::new_unchecked(1);
        let sender_connection = ConnectionId::new_unchecked(2);
        let mut admission = Admission::default();
        let none = Endpoints::default();
        admission.remotes.insert(
            provider_connection,
            (
                provider,
                "/ip6/2001:db8::1/tcp/7070".parse().unwrap(),
                false,
            ),
        );
        admission.remotes.insert(
            sender_connection,
            (sender, "/ip6/2001:db8::2/tcp/7070".parse().unwrap(), false),
        );
        let mut record = kad::ProviderRecord {
            key: kad::RecordKey::new(&dyapp_p2p_net::SEARCH_KEY),
            provider,
            expires: None,
            addresses: vec!["/ip6/2001:db8::1/udp/7071/quic-v1".parse().unwrap()],
        };
        assert!(admission.provider_allowed(&none, &record, 0));
        // Without the node-ID proof of work, or for another key, nothing is stored.
        assert!(!admission.provider_allowed(&none, &record, 255));
        let turn = kad::ProviderRecord {
            key: kad::RecordKey::new(&dyapp_p2p_net::TURN_KEY),
            ..record.clone()
        };
        assert!(admission.provider_allowed(&none, &turn, 0));
        let other = kad::ProviderRecord {
            key: kad::RecordKey::new(&b"test"),
            ..record.clone()
        };
        assert!(!admission.provider_allowed(&none, &other, 0));
        // The sender's connection cannot substantiate the advertised provider's address.
        record
            .addresses
            .push("/ip6/2001:db8::2/tcp/7070".parse().unwrap());
        assert!(!admission.provider_allowed(&none, &record, 0));
        record.addresses = vec!["/dns/example.test/tcp/7070".parse().unwrap()];
        assert!(!admission.provider_allowed(&none, &record, 0));
        record.addresses.clear();
        assert!(!admission.provider_allowed(&none, &record, 0));
        record
            .addresses
            .push("/ip6/2001:db8::1/tcp/7070".parse().unwrap());
        admission.closed(provider_connection, provider, true);
        assert!(!admission.provider_allowed(&none, &record, 0));
    }

    /// The proof-of-work bits are process-wide: tests that zero them run one at a time.
    static POW: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Zeroes the proof-of-work bits until dropped.
    struct ZeroPow(u32, #[allow(dead_code)] std::sync::MutexGuard<'static, ()>);
    impl Drop for ZeroPow {
        fn drop(&mut self) {
            dyapp_p2p_net::set_id_pow_bits(self.0);
        }
    }

    fn zero_pow() -> ZeroPow {
        let guard = POW.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let bits = dyapp_p2p_net::id_pow_bits();
        dyapp_p2p_net::set_id_pow_bits(0);
        ZeroPow(bits, guard)
    }

    #[test]
    fn a_provider_flood_under_other_keys_leaves_the_search_key_room() {
        let _pow = zero_pow();
        let mut swarm =
            dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        let provider = PeerId::random();
        let address: Multiaddr = "/ip6/2001:db8::1/tcp/7070".parse().unwrap();
        let mut admission = Admission::default();
        admission.remotes.insert(
            ConnectionId::new_unchecked(1),
            (provider, address.clone(), false),
        );
        let record = |key: &[u8]| kad::ProviderRecord {
            key: kad::RecordKey::new(&key),
            provider,
            expires: None,
            addresses: vec![address.clone()],
        };
        // More keys than the MemoryStore's default max_provided_keys (1024).
        for i in 0..1100u32 {
            admission.add_provider(&mut swarm, record(&i.to_be_bytes()));
        }
        admission.add_provider(&mut swarm, record(dyapp_p2p_net::SEARCH_KEY));
        let store = swarm.behaviour_mut().kad.store_mut();
        assert!(store
            .providers(&kad::RecordKey::new(&0u32.to_be_bytes()))
            .is_empty());
        let key = kad::RecordKey::new(&dyapp_p2p_net::SEARCH_KEY);
        assert_eq!(store.providers(&key).len(), 1);
    }

    #[test]
    fn only_the_matching_claim_connection_routes_an_alternative_address() {
        let _pow = zero_pow();
        let mut swarm =
            dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        let peer = PeerId::random();
        let other = PeerId::random();
        let claimed: Multiaddr = "/ip4/198.51.100.1/tcp/7070".parse().unwrap();
        let id = ConnectionId::new_unchecked(7);
        let unrelated = ConnectionId::new_unchecked(8);
        let endpoint = ConnectedPoint::Dialer {
            address: claimed.clone(),
            role_override: Endpoint::Dialer,
            port_use: PortUse::New,
        };
        let mut admission = Admission::default();
        admission.roles.insert(peer, (true, false));
        assert!(admission
            .claims
            .claim(Instant::now(), peer, claimed.clone()));
        admission.claims.start(Instant::now(), |_, _| Some(id));

        admission.established(&mut swarm, unrelated, peer, &endpoint);
        assert!(dyapp_p2p_net::known_peers(&mut swarm).is_empty());
        admission.established(&mut swarm, id, other, &endpoint);
        assert!(dyapp_p2p_net::known_peers(&mut swarm).is_empty());
        admission.established(&mut swarm, id, peer, &endpoint);
        assert!(dyapp_p2p_net::known_peers(&mut swarm).is_empty());

        let next = ConnectionId::new_unchecked(9);
        let second: Multiaddr = "/ip4/198.51.100.2/tcp/7070".parse().unwrap();
        assert!(admission.claims.claim(Instant::now(), peer, second.clone()));
        admission.claims.start(Instant::now(), |_, _| Some(next));
        admission.failed(&mut swarm, next);
        admission.established(&mut swarm, next, peer, &endpoint);
        assert!(dyapp_p2p_net::known_peers(&mut swarm).is_empty());

        let success = ConnectionId::new_unchecked(10);
        let third: Multiaddr = "/ip4/198.51.100.3/tcp/7070".parse().unwrap();
        assert!(admission.claims.claim(Instant::now(), peer, third.clone()));
        admission.claims.start(Instant::now(), |_, _| Some(success));
        let verified_endpoint = ConnectedPoint::Dialer {
            address: third.clone(),
            role_override: Endpoint::Dialer,
            port_use: PortUse::New,
        };
        admission.established(&mut swarm, success, peer, &verified_endpoint);
        let mut expected = third;
        expected.push(Protocol::P2p(peer));
        assert_eq!(dyapp_p2p_net::known_peers(&mut swarm), vec![expected]);
    }
}
