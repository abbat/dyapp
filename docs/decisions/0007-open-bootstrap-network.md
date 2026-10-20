# 0007. Open bootstrap network with DHT and no commercial relays

- **Status:** Accepted
- **Date:** 2026-10-13

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
- **Nodes are untrusted.** Clients and nodes rely on a node only for availability, and only
  through redundancy (several replicas on several nodes). Everything else is checked by the
  recipient:
  - content: the owner's signature, and end-to-end encryption for private data;
  - keys a node hands out (identity keys, MLS key packages): a node's claim until the user
    verifies them out of band;
  - claims about the network (addresses, roles, proof of work, inventory): verified locally or
    used only as hints.
- **Nodes get only what routing and storage need:** public profiles, ciphertext and routing
  metadata ([Privacy](../security/privacy.md)).

## Consequences

- Withholding, eclipse and Sybil attacks remain possible; the measures that make them harder
  are in [ADR 0008](0008-sybil-and-eclipse-defences.md).
- A node that withholds data is detected only by asking other replicas; a key handed out by a
  node protects against that node only after out-of-band verification
  ([ADR 0004](0004-identity-keys.md)), which is not implemented yet.
  Threat classes and their status: [Threat Model](../security/threat-model.md#threat-classes).
- Node operators see public profiles and routing metadata by design
  ([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)).
- Calls behind strict NATs depend on volunteers running TURN nodes.
- Independent networks with their own names share the code but not the network:
  [ADR 0017](0017-network-names.md).
- Implemented: `dyappd` is a libp2p node in the Kademlia DHT (`/dyapp/kad`, the store role's
  key space) and announces its roles, TURN included, through identify
  ([Roles and discovery](../architecture/bootstrap.md#roles-and-discovery)). Separate key spaces
  for the other roles are planned.
