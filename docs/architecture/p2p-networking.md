# P2P Networking Architecture

## Current code

`rust/p2p-net` builds a [rust-libp2p](https://github.com/libp2p/rust-libp2p) 0.57 node
([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md),
[ADR 0014](../decisions/0014-libp2p-only-node-protocol.md)). `build_swarm(keypair, mode)` returns a
`Swarm` with the settings below. `build_configured_swarm(keypair, SwarmOptions)` accepts the
mode, connection limits, stream cap, memory limit and search role; `dyappd` uses it.
`build_swarm` and `build_limited_swarm` retain their existing interfaces and delegate to it.
Default options use `Mode::Auto`, no connection or memory cap, 100 streams and no search role.

`SwarmOptions.incoming_groups` optionally enables `IncomingGroupLimits::new(max, ipv4_prefix,
ipv6_prefix)`: a positive connection budget shared by pending and established incoming
connections. It groups the first remote IP by masks 0..32 and 0..128; IPv4-mapped IPv6 stays
IPv6, matching the node's existing grouping. Non-IP endpoints share one bounded group.
A slot survives handshake completion and is released on handshake failure, a refusal by any
composed behaviour, or connection close. Outgoing connections are exempt.
`IncomingGroupGuard::take_denied()` reads and resets refusals. Legacy constructors leave
this optional guard disabled; **bootstrap configuration wiring and real TCP acceptance remain
planned**. Deterministic callback tests verify the shared budget and cleanup.

| Part | Setting |
|------|---------|
| Transports | QUIC (`/udp/<port>/quic-v1`) and TCP with Noise and Yamux |
| Kademlia | protocol `/dyapp/kad`, in-memory record store |
| identify | protocol `/dyapp`; tells peers their observed address and fills the Kademlia routing table: `dyappd` adds the claimed listen addresses of every peer that serves `/dyapp/kad`, unfiltered |
| AutoNAT | v1, as client and server; confirms external addresses |
| Dialing | Happy Eyeballs (RFC 8305), libp2p smart dial: every address of a peer is dialled, QUIC before TCP and IPv6 before IPv4; IPv4 starts 250 ms after IPv6 on public addresses, 30 ms on private ones, and the first connection wins. A failed IPv6 dial does not start IPv4 sooner, and an IPv6 failure is not remembered, so every dial tries IPv6 again |
| Idle connections | closed after 60 s |
| Limits | connection limits (none by default), 100 concurrent streams per connection and protocol |

`Mode::Auto` makes the node a DHT server only once it has a confirmed external address (from AutoNAT
or `Swarm::add_external_address`); until then it is a DHT client. `Mode::Client` never serves DHT
queries; mobile apps use it.

The keypair is the node's transport key, separate from the user's identity key
([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md)).

The unit test starts a server on QUIC loopback and checks that a client adds it to its routing
table.

**Joining.** The transports resolve DNS names, so a `/dns/` address dials like an IP one and
tries the resolved addresses in turn. `dyappd` takes `host:port` or `host:port/<peer ID>` seeds
and dials each over `/tcp/<port>` and `/udp/<port>/quic-v1`. An optional peer ID appends
`/p2p/<id>` to both addresses and rejects a different transport key; mismatches are logged at
warn. Plain seeds still learn the ID on connect. Bad peer IDs and raw multiaddrs are refused at
startup. `join` dials every seed and adds
the cached peers (`.../p2p/<id>` addresses) to the routing table; Kademlia then bootstraps on its
own. `known_peers` lists the routing table for the cache. `dyappd` reads seeds from the
`seeds` config key and keeps the cache in `<storage.dir>/peers`, written hourly with store
maintenance, so a restart joins without any one seed. The last 3 distinct peers it dialled
that answered identify as Kademlia servers with the ID proof of work are its anchors, saved
hourly to `<storage.dir>/anchors` and dialled first on the next start, before the cache fills
the routing table and before the seeds, so an attacker who floods the cache does not decide the
first peers. A unit test checks the anchor list. It dials the seeds again on every
maintenance run (the first at start) and every 5 minutes while its routing table is empty. A unit
test joins through a seed, then joins a second swarm from the first one's cache alone.

**Routing defences** ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)). Lookups use
disjoint query paths. Kademlia inserts nothing on its own (`BucketInserts::Manual`): every peer,
whether dialled, found by a lookup, cached or announced by identify, goes through `add_peer`.
It refuses a peer whose ID lacks the proof of work: SHA-256 of the peer ID bytes must start with
`ID_POW_BITS` (22) zero bits, overridable per process by `set_id_pow_bits` for test networks.
Such a peer is still served but never routed to, and `dyappd` sends it no replica.
`generate_pow_keypair` searches a key on every CPU. `add_peer` also refuses a peer when its bucket already holds 2 peers of the same IP group, or the table
holds 10. An IP group is an IPv4 /24, an IPv6 /48, or one DNS name. With
`set_distinct_groups` on (the default) a bucket also takes at most one peer per /16 (IPv6 /32,
or one DNS name), so the peers Kademlia dials for one key range sit in distinct networks;
`dyappd` sets it from `network.distinct_outbound_groups`. Call `route` on every swarm
event so that the peers Kademlia finds routable reach the filter. A full bucket keeps its
connected peers: libp2p tests before it evicts, dialling the oldest disconnected peer and
replacing it with the new one only if it is still disconnected after 60 s, so long-lived peers
stay in it. A unit test
checks the limits. Addresses claimed through identify are not verified by a dial.

Not implemented yet:

- Only `dyappd` runs the node, serving `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`,
  `/dyapp/mailbox-push` and, with the media role, `/dyapp/media`
  ([bootstrap](bootstrap.md#served-protocol)); `test-peer` is the only client, and the FFI does
  not expose `p2p-net`.
- Local reputation lives in `dyappd`, not here ([bootstrap](bootstrap.md#rate-limiting)).
- No relay or DCUtR hole punching.
- No seed list is built in: operators set `seeds` themselves, and no client joins yet.
- No signal protocol yet; signal envelopes are planned in
  [Bootstrap — Protocol](bootstrap.md#protocol).

## Dial filtering

`DialFilter::new(Vec<IpNet>)` describes denied outgoing IP ranges; its empty default allows
all addresses. It examines the first IP component anywhere in a multiaddress and treats
IPv4-mapped IPv6 as IPv4. `DenyDial<T>` refuses a denied dial before calling the inner
transport and counts refusals; clones share the counter and `take_denied()` resets it.
Listening, inbound transport events and allowed dials are delegated unchanged.

The wrapper and address tests are implemented. **Placing wrappers around raw TCP and QUIC
beneath DNS, configuring the bootstrap denylist and reporting its counters remain planned**.
A DNS-only address has no IP for the predicate to check; resolver placement is required to
refuse names resolving to denied IPs. Current swarm builders do not yet apply this wrapper.

## Protocol namespaces

`Protocols::public()` describes the existing public IDs and gossip defaults.
`Protocols::new(name)` validates at most 64 printable ASCII bytes, with no leading or
trailing space; empty means public, and case is preserved. Named IDs use
`/dyapp/net-<ns>/<protocol>`, where `ns` is the first eight bytes of
SHA-256(`dyapp/network/` || name) in hex. The identify version is `/dyapp/net-<ns>`,
the gossip prefix is `/dyapp/net-<ns>/meshsub`, and search topics are
`/dyapp/net-<ns>/profiles/<n>`. The reserved digest ID receives the same namespace
but has no behaviour yet; `all()` lists the nine existing protocol IDs.

The value type and name validation are implemented. **Passing it into swarm builders,
bootstrap network configuration, storage guards and peer isolation remain planned**;
current builders continue to use the public constants.

## Planned

- **First nodes:** a list built into the client and DNS seed names from several operators; the
  client asks several seeds at once and keeps the peer cache described above. Each built-in seed
  carries its peer ID, and a `dyappd` seed may (`host:port/<peer ID>`); a node with another key
  is refused ([threat model](../security/threat-model.md#mitm-and-key-verification)).
- **NAT traversal:** direct connection first, then DCUtR hole punching through a relay, then the
  relay itself. Video uses its own WebRTC stack ([Video](video.md)).
- **Addresses:** link-local addresses are not announced. A host whose IPv6 does not work is
  remembered for a while, so its dials go straight to IPv4.
- **Search and storage:** served by bootstrap nodes over libp2p request-response protocols
  ([Bootstrap](bootstrap.md)). Profiles are public signed records
  ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); search filters
  are described in [ADR 0012](../decisions/0012-private-p2p-interactions.md).
- **Security:** Noise authenticates the transport key; MLS carries end-to-end keys and user
  authentication ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)). Direct
  connections reveal IP addresses to the other peer by design ([Privacy](../security/privacy.md)).
