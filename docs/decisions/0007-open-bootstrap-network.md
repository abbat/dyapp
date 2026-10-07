# 0007. Open bootstrap network with DHT and no commercial relays

- **Status:** Accepted
- **Date:** 2026-10-06

## Context

Bootstrap nodes store offline data, serve the profile index and relay calls (TURN). Someone has to
run them, and whoever controls them can control the network.

## Decision

- **Open network from the start:** anyone may run a bootstrap node.
- **A DHT is expected** for node discovery (including TURN-capable nodes); its design is in
  [ADR 0008](0008-sybil-and-eclipse-defences.md).
- **No commercial relay providers** (Twilio and similar). TURN is provided only by network nodes.
- **Baseline defence against fake nodes:** all data is signed by its owner and verified by every
  recipient, so a node can drop or withhold data but cannot forge it.

## Consequences

- Withholding, eclipse and Sybil attacks remain possible; the measures that make them harder
  are in [ADR 0008](0008-sybil-and-eclipse-defences.md).
- Node operators see public profiles and routing metadata by design
  ([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)).
- Calls behind strict NATs depend on volunteers running TURN nodes.
- Not implemented: the bootstrap is a single REST server, not a DHT node.
