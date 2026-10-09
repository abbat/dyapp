# 0008. rust-libp2p with layered Sybil and eclipse defences

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

The bootstrap network is open to anyone ([ADR 0007](0007-open-bootstrap-network.md)), and
signatures stop forged data, not hidden data. The offline store
([ADR 0009](0009-message-delivery-and-storage.md)) is an LRU cache that spam can evict, and likes
and view signals ([ADR 0012](0012-private-p2p-interactions.md)) add unsolicited writes. The current
`rust/p2p-net` is a stub on `quinn`; there is no DHT.

What other networks show:

- **IPFS (go-libp2p):** any node could be eclipsed in under an hour
  ([Prünster et al., USENIX Security 2022](https://www.usenix.org/conference/usenixsecurity22/presentation/prunster));
  the fix caps routing-table peers per IP group
  ([IPFS blog](https://blog.ipfs.tech/2020-10-30-dht-hardening/)). Attacks on a single key still
  work after it ([arXiv 2505.01139](https://arxiv.org/abs/2505.01139)).
- **rust-libp2p Kademlia:** no IP diversity; disjoint query paths exist but are off by default;
  manual bucket insertion allows a custom filter ([docs](https://docs.rs/libp2p-kad)).
- **Ethereum discv5:** at most 2 nodes per /24 in a bucket and 10 in the table
  ([devp2p #109](https://github.com/ethereum/devp2p/issues/109)); one Sybil per bucket still
  eclipses ([Henningsen et al.](https://arxiv.org/abs/1908.10141)).
- **Bitcoin Core:** outbound peers from distinct /16 groups, anchor connections kept across
  restarts, test-before-evict ([Heilman et al. 2015](https://eprint.iacr.org/2015/263.pdf)).
- **S/Kademlia:** proof-of-work on node IDs and lookups over disjoint paths; 99% of lookups succeed
  with 20% adversarial nodes
  ([Baumgart, Mies 2007](https://telematics.tm.kit.edu/publications/Files/267/SKademlia_2007.pdf)).

## Decision

Use **rust-libp2p** (Kademlia, QUIC, Noise, AutoNAT, DCUtR) and add the defences it lacks:

1. **Cost of entry.** A node ID is the hash of a node key, separate from the user's identity key
   ([ADR 0006](0006-transport-keys-separate-from-identity.md)), and carries a proof-of-work
   (S/Kademlia static puzzle: SHA-256 of the public key starts with a fixed number of zero bits,
   so no nonce travels with the peer ID). Difficulty is a protocol constant, about one minute on
   2 vCPU, paid once. The operator creates the key explicitly with a `keygen` subcommand and keeps
   it; a node does not start without one. Peers without the proof (phones, older nodes) are still
   served, but never enter the routing table or receive replicas.
2. **Routing-table diversity.** At most 2 peers from one /24 (IPv6: /48) per bucket and 10 per
   table, enforced by a filter on manual bucket insertion. Outbound connections go to distinct /16
   (IPv6: /32) groups; on by default, an operator who trusts the network may turn it off.
   AS-level grouping (like Bitcoin's asmap) is deferred.
3. **Lookups** use disjoint query paths; a result is accepted only after its signature verifies.
4. **Replica placement.** Replica *i* of a record ([ADR 0009](0009-message-delivery-and-storage.md))
   is stored at the nodes closest to H(key ‖ i), so hiding the record needs several eclipsed
   points, not one.
5. **Local reputation only.** Each node and client scores peers by what it observes (answers,
   valid signatures, observed uptime), trusts a peer with storage only after it has seen it for a
   set time (default 1 hour, configurable), and tests a peer before evicting it from a full
   bucket. There is no global reputation and no network-wide vote.
6. **First nodes** come from configured seeds (multiaddrs or DNS seeds via `/dnsaddr`) and anchor
   nodes: the last 2-3 outbound peers, saved and dialled first after a restart. The default seed
   list is empty until independent operators run public seeds. The client asks several seeds at
   once.
7. **Store protection.** The bootstrap store keeps a separate pool per data type, so likes and view
   signals cannot evict messages, with quotas per sender key and per IP group. A hashcash stamp on
   unsolicited writes (likes, view signals, messages without a match) is best effort: the envelope
   reserves an optional stamp field, nodes may use it to prioritise under load, and none require it
   in v1. The exception is search: a profile carries a proof of work bound to its owner's key,
   computed once when the key is created, and search nodes index only profiles that have it; each
   node sets the minimum difficulty it accepts. Without it free keys could flood the search index
   with fake profiles and evict real ones.

## Consequences

- Targeted eclipse of one key becomes much more expensive but stays possible.
- Moderation stays local in v1 ([ADR 0012](0012-private-p2p-interactions.md)).
- Proof-of-work costs an operator about a minute once per node key; a node key without it must be
  replaced, which changes the node's peer ID.
- Some defences are our code on top of libp2p: the diversity filter, node-ID proof-of-work, anchors,
  local reputation, replica placement and store pools.
- Implemented in `rust/p2p-net`: disjoint lookups, the node-ID proof of work (22 bits, made by
  `dyappd keygen`), the per-bucket and per-table IP-group limits on manual inserts and one peer
  per /16 in each bucket ([p2p-networking.md](../architecture/p2p-networking.md)). The node joins
  through seeds (`/dnsaddr` too), dials them again periodically and caches peers across restarts.
- Not implemented (planned): anchors, test-before-evict, the storage trust delay and reputation
  scores.
  The bootstrap store has no per-type pools. Nodes rate-limit per peer ID, IP group and sender
  key and keep a local, unshared ban score per peer
  ([bootstrap.md](../architecture/bootstrap.md#rate-limiting)).
