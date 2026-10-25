//! Seed rows shared by dyappd and clients: `host:port` or `host:port/<peer ID>`, with an IP,
//! `[IPv6]` or DNS host, dialed over both TCP and QUIC.

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use std::net::SocketAddr;

/// Why a seed row cannot be dialed; callers word it for their own audience.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedError {
    /// Not `host:port`: a multiaddr, a missing or bad port, or an unbracketed IPv6.
    Address,
    /// The part after `/` is not a peer ID.
    PeerId,
}

/// The TCP and QUIC addresses of one seed row; a pinned row ends both with `/p2p/<peer ID>`.
pub fn parse(row: &str) -> Result<[Multiaddr; 2], SeedError> {
    if let Some((host, peer)) = row.split_once('/') {
        let addresses = parse(host)?;
        let peer: PeerId = peer.parse().map_err(|_| SeedError::PeerId)?;
        return Ok(addresses.map(|address| address.with(Protocol::P2p(peer))));
    }
    let host = match row.parse::<SocketAddr>() {
        Ok(socket) => Multiaddr::from(socket.ip()),
        Err(_) => Multiaddr::empty().with(Protocol::Dns(
            row.rsplit_once(':')
                .map(|(host, _)| host)
                .filter(|host| !host.is_empty() && !host.contains([':', '/', '[']))
                .ok_or(SeedError::Address)?
                .into(),
        )),
    };
    let port = row
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .ok_or(SeedError::Address)?;
    Ok([
        host.clone().with(Protocol::Tcp(port)),
        host.with(Protocol::Udp(port)).with(Protocol::QuicV1),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_to_tcp_and_quic_and_errors_are_typed() {
        let [tcp, quic] = parse("seed.example:7070").unwrap();
        assert_eq!(tcp.to_string(), "/dns/seed.example/tcp/7070");
        assert_eq!(quic.to_string(), "/dns/seed.example/udp/7070/quic-v1");
        let [tcp, _] = parse("[2001:db8::7]:33445").unwrap();
        assert_eq!(tcp.to_string(), "/ip6/2001:db8::7/tcp/33445");
        let peer = PeerId::random();
        let [_, quic] = parse(&format!("192.0.2.7:1/{peer}")).unwrap();
        assert_eq!(
            quic.to_string(),
            format!("/ip4/192.0.2.7/udp/1/quic-v1/p2p/{peer}")
        );
        for bad in [
            "",
            "seed.example",
            "seed.example:",
            "seed.example:70000",
            ":7070",
            "2001:db8::7:7070",
            "[2001:db8::7]",
            "/dns/seed.example/tcp/7070",
        ] {
            assert_eq!(parse(bad), Err(SeedError::Address), "{bad:?}");
        }
        for bad in [
            "seed.example:7070/",
            "seed.example:7070/x",
            &format!("a:1/{peer}/x"),
        ] {
            assert_eq!(parse(bad), Err(SeedError::PeerId), "{bad:?}");
        }
    }
}
