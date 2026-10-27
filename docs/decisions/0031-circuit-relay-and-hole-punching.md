# 0031. Circuit relay on bootstrap nodes and hole punching

- **Status:** Accepted
- **Date:** 2026-10-27

## Context

Most clients and desktop nodes ([ADR 0030](0030-desktop-full-node.md)) sit behind NAT. They reach
public `dyappd` nodes over QUIC or TCP, so mailboxes already work through store-and-forward, but
nobody can open a connection to them: a desktop node behind NAT serves only the connections it
opens, and two such peers cannot talk directly. [ADR 0008](0008-sybil-and-eclipse-defences.md)
names DCUtR among the rust-libp2p parts we use; [ADR 0007](0007-open-bootstrap-network.md) and
[ADR 0013](0013-video-calls-one-to-one.md) cover call media through TURN only. Hole punching
needs a third peer both sides reach to coordinate the attempt, and some NATs cannot be punched.
A relay carries other people's traffic, so it needs limits.

## Decision

Use libp2p circuit relay v2 and DCUtR (the `relay` and `dcutr` features of the `libp2p` crate).

- **Relays.** `dyappd` gains a `relay` role, off by default. A node with it serves the circuit
  relay v2 hop protocol with libp2p's limits, set in the config: reservations in total and per
  peer and IP, reservation duration, circuits in total and per peer, and per-circuit duration and
  bytes. The defaults keep circuits short (2 minutes, 128 KiB), enough to coordinate a hole punch
  and to carry small exchanges, not bulk data.
- **Peers behind NAT.** Every swarm runs the relay client and DCUtR. A peer that AutoNAT reports
  private reserves a slot on a relay among its connected nodes and announces the circuit address.
  A peer dialling it tries the direct addresses first, then the circuit; on a relayed connection
  both sides attempt DCUtR, and a direct connection replaces the circuit when it succeeds.
- **Fallbacks.** QUIC stays first and TCP stays the fallback where UDP is blocked; a relayed
  connection is the last resort. Circuit addresses never enter the DHT routing tables: the address
  admission rules ([bootstrap](../architecture/bootstrap.md#protocol)) keep rejecting them, so a
  peer that cannot be punched stays reachable only through its circuit.
- **Acceptance.** A test runs a relay and two peers that accept no direct connections: they
  connect through the circuit and exchange a message.

## Consequences

- Desktop nodes behind NAT and peers that cannot be punched become reachable; the relay limits
  cap what a relay operator pays for strangers.
- The `libp2p` crate gains two features and their crates; every swarm carries two more behaviours.
- Relayed connections share the relay's IP, so per-IP limits and incoming groups count them under
  the relay, not the far peer; the far peer's own limits still apply at the relay.
- The hop protocol is the shared libp2p one, so a relay serves any network name; identify on the
  relayed connection still separates networks ([ADR 0017](0017-network-names.md)).
- A peer behind a NAT that cannot be punched depends on volunteers running the relay role, as calls
  depend on TURN nodes.
