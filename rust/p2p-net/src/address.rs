use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;

/// Whether `address` routes through a relay circuit.
pub fn is_circuit(address: &Multiaddr) -> bool {
    address.iter().any(|p| p == Protocol::P2pCircuit)
}

/// Whether two direct addresses name exactly the same IP, independent of port and transport.
/// DNS, relayed routes and multiple IP components cannot prove a direct remote endpoint.
pub fn same_ip(observed: &Multiaddr, claimed: &Multiaddr) -> bool {
    fn direct_ip(address: &Multiaddr) -> Option<std::net::IpAddr> {
        let mut parts = address.iter();
        let ip = match parts.next()? {
            Protocol::Ip4(ip) => ip.into(),
            Protocol::Ip6(ip) => ip.into(),
            _ => return None,
        };
        if parts.any(|p| {
            matches!(
                p,
                Protocol::Ip4(_)
                    | Protocol::Ip6(_)
                    | Protocol::Dns(_)
                    | Protocol::Dns4(_)
                    | Protocol::Dns6(_)
                    | Protocol::Dnsaddr(_)
                    | Protocol::P2pCircuit
            )
        }) {
            return None;
        }
        Some(ip)
    }
    direct_ip(observed).is_some_and(|ip| direct_ip(claimed) == Some(ip))
}

/// The IP group of `address`: /24 (IPv6 /48), or /16 (IPv6 /32) when `wide`; a DNS name is its
/// own group.
pub fn ip_group(address: &Multiaddr, wide: bool) -> Option<String> {
    address.iter().find_map(|p| match p {
        Protocol::Ip4(ip) => {
            let [a, b, c, _] = ip.octets();
            Some(if wide {
                format!("{a}.{b}")
            } else {
                format!("{a}.{b}.{c}")
            })
        }
        Protocol::Ip6(ip) => {
            let s = ip.segments();
            Some(if wide {
                format!("{:x}:{:x}", s[0], s[1])
            } else {
                format!("{:x}:{:x}:{:x}", s[0], s[1], s[2])
            })
        }
        Protocol::Dns(h) | Protocol::Dns4(h) | Protocol::Dns6(h) | Protocol::Dnsaddr(h) => {
            Some(h.to_string())
        }
        _ => None,
    })
}
