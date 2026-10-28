# 0017. Network names for independent networks

- **Status:** Accepted
- **Date:** 2026-10-20

## Context

Independent networks (a city, an organisation) run the same code as the public network
([ADR 0007](0007-open-bootstrap-network.md)). Without a boundary, one seed or anchor that points
at another network merges the two: the DHTs, gossip and stored data mix. The name of such a
network is not a secret: an unbounded group of people knows it. Peer authentication and
resistance to a deliberate intruder are out of scope; the goal is that networks never merge by
accident, and that a peer of another network is cut off before any decoding, proof-of-work or
storage work.

## Decision

`[network] name` names the network: printable ASCII `0x20`-`0x7E`, at most 64 bytes, no leading
or trailing space, compared as bytes; empty is the public network. Its namespace `ns` is the first
8 bytes of SHA-256(`"dyapp/network/"` ‖ name) in lowercase hex.

- Every stream protocol, gossip topic and the gossipsub protocol get a `net-<ns>` segment:
  `/dyapp/net-<ns>/<p>`, `/dyapp/net-<ns>/profiles/<n>`, `/dyapp/net-<ns>/meshsub`. The identify
  `protocol_version` is `/dyapp/net-<ns>`. The public network keeps `/dyapp/<p>`, `/meshsub` and
  `/dyapp` byte for byte, so deployed nodes need no migration. IDs come from `Protocols` in
  `rust/p2p-net`, so a new protocol gets the namespace.
- Identify and AutoNAT stay shared. A node disconnects a peer whose identify `protocol_version`
  differs from its own and adds it to no DHT and no anchor list. Before identify, a request of
  another network fails in multistream-select and is never decoded.
- `storage.dir` is bound to one network by its `network` file; a node with another name refuses
  to start on it.

Rejected: libp2p pnet and a Noise prologue need a secret key such a group cannot keep, and pnet
does not work with QUIC; a network field in every message is checked only after decoding.

## Consequences

- Two networks that pick the same name merge; choosing the name is the operator's job.
- An AutoNAT dial-back answer is true in any network: AutoNAT is the known shared exception.
- TURN relays are reachable by address regardless of the name (TURN auth is separate).
- Moving a node to another network needs a new `storage.dir`.
- Client apps do not pass a network name yet: they will when they get libp2p (planned).
