# P2P Networking Architecture

## Current code

`rust/p2p-net` builds a [rust-libp2p](https://github.com/libp2p/rust-libp2p) 0.57 node
([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md),
[ADR 0014](../decisions/0014-libp2p-only-node-protocol.md)). `build_swarm(keypair, mode)` returns a
`Swarm` with:

| Part | Setting |
|------|---------|
| Transports | QUIC (`/udp/<port>/quic-v1`) and TCP with Noise and Yamux |
| Kademlia | protocol `/dyapp/kad`, in-memory record store |
| identify | protocol `/dyapp`; tells peers their observed address and fills the Kademlia routing table |
| AutoNAT | v1, as client and server; confirms external addresses |
| Idle connections | closed after 60 s |

`Mode::Auto` makes the node a DHT server only once it has a confirmed external address (from AutoNAT
or `Swarm::add_external_address`); until then it is a DHT client. `Mode::Client` never serves DHT
queries; mobile apps use it.

The keypair is the node's transport key, separate from the user's identity key
([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md)).

The unit test starts a server on QUIC loopback and checks that a client adds it to its routing
table.

Not implemented yet:

- Only `dyapp-node` runs the node, serving `/dyapp/node` and `/dyapp/profile`
  ([bootstrap](bootstrap.md#served-protocol)); `test-peer` is the only client, and the FFI does
  not expose `p2p-net`.
- No node-ID proof of work, routing-table IP diversity filter, disjoint lookups, anchors or local
  reputation ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)).
- No relay, DCUtR hole punching or DNS seeds.
- No mailbox, signal, media or search protocols; they are designed in
  [Bootstrap — Protocol](bootstrap.md#protocol).

## Planned

- **First nodes:** a list built into the client, `/dnsaddr` seeds from several operators, and anchor
  nodes saved from the previous session; the client asks several seeds at once.
- **NAT traversal:** direct connection first, then DCUtR hole punching through a relay, then the
  relay itself. Video uses its own WebRTC stack ([Video](video.md)).
- **Addresses:** IPv6 and IPv4 dual-stack; link-local addresses are not announced.
- **Search and storage:** served by bootstrap nodes over libp2p request-response protocols
  ([Bootstrap](bootstrap.md)). Profiles are public signed records
  ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); search filters
  are described in [ADR 0012](../decisions/0012-private-p2p-interactions.md).
- **Security:** Noise authenticates the transport key; MLS carries end-to-end keys and user
  authentication ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)). Direct
  connections reveal IP addresses to the other peer by design ([Privacy](../security/privacy.md)).
