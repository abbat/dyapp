# Threat Model

Who can attack the network and its nodes, what they are after, and which defence covers each
threat. **Status** says whether the defence exists in code today; [Encryption & Security
Status](encryption.md) remains the source of truth for crypto claims and
[Privacy](privacy.md) for who sees which field.

## Assets

| Asset | Property we want |
|-------|------------------|
| Message content | Only the recipient's devices read it |
| Public profile | Only the owner publishes or changes it; anyone may read it |
| Mailbox | Only the device that owns it reads or deletes its envelopes |
| Identity key, device keys | Never leave the device |
| Metadata (who writes to whom, when, from which IP) | As little as possible leaks; full hiding is not a goal |
| Availability | A user can publish, fetch and deliver while some honest nodes are reachable |
| Node resources (disk, traffic, CPU, connections) | One abuser cannot exhaust them or knock the node over |

## Attackers

| Attacker | Can |
|----------|-----|
| **Node operator** | Read everything stored on or sent to their node; drop, delay or withhold data; serve an older signed profile; log IPs and timing |
| **Network observer** (ISP, Wi-Fi) | See connection endpoints, sizes and timing; tamper with packets |
| **Malicious client or peer** | Send any request to any node: floods, oversized or malformed messages, forged signatures, replayed requests, writes into strangers' mailboxes |
| **Sybil** | Create unlimited libp2p peer IDs and identity keys for free, run many nodes, surround a key in the DHT (eclipse) |
| **Compromised device** | Everything that device's keys allow |

## Threats and defences

| Threat | Defence | Status |
|--------|---------|--------|
| Observer reads or tampers with traffic | libp2p QUIC (TLS 1.3) and TCP + Noise | ✅ Implemented ([p2p-net](../architecture/p2p-networking.md)) |
| Operator or observer reads messages | MLS end-to-end encryption ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)) | ❌ Planned: payload crypto is a stub |
| Forged or rolled-back profile | Ed25519 signature by the owner; a node rejects an older version | ✅ Implemented; a node can still serve an older version it kept |
| Another client reads or empties a mailbox | Fetch and ack signed by the device key over a nonce issued for this connection | ✅ Implemented ([mailboxes](../architecture/bootstrap.md#mailboxes)) |
| Replayed mailbox request | The nonce is bound to one libp2p connection; other requests are idempotent | ✅ Implemented |
| Spam into a mailbox | Size limits per envelope and mailbox, puts limited per sender key and IP group | ✅ Implemented; the sender key is free, so the IP group is the real bound |
| Request floods | Token buckets per peer ID and IP group, local ban after repeated refusals | ✅ Implemented ([rate limiting](../architecture/bootstrap.md#rate-limiting)) |
| Disk, traffic or connection exhaustion | Store size caps, free-space floor, byte rate, connection, stream and memory limits | ✅ Implemented ([resource guards](../architecture/bootstrap.md#resource-guards)) |
| Oversized or malformed messages | 2 MiB wire cap, protobuf decoding, unknown variants answered `UNSUPPORTED` | ✅ Implemented |
| Operator withholds or drops data | R = 5 replicas on independent nodes; the client tries another replica ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)) | ❌ Planned: a node stores only what it is sent |
| Abuser keeps using one node | Operator deny list of peer IDs, IP groups and key hashes | ✅ Implemented ([deny list](../architecture/bootstrap.md#deny-list)); a key is free to regenerate, so IP-group quotas carry the rest |
| Sybil eclipse of a key or a node | Node-ID proof of work, routing-table IP diversity, disjoint lookups, replicas at H(key ‖ i), local reputation ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)) | ❌ Planned: the DHT is plain Kademlia |
| No way in without one seed | Several seeds, DNS seed names, a cache of peers seen before | ⚠️ Partial: seeds, the node's peer cache and anchors exist; no list ships ([joining](../architecture/p2p-networking.md)) |
| Search index flooded with fake profiles | Profile proof of work bound to the owner's key ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)) | ❌ Planned: there is no search |
| Metadata: who talks to whom | Mailbox addresses are device keys, not user IDs; delivery receipts travel as ordinary encrypted messages | ⚠️ Partial: the operator still sees which sender key writes to which mailbox, and IPs ([privacy](privacy.md)) |
| Stolen identity or device key | Keys stay on the device, separate transport keys ([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md)); MLS post-compromise security | ❌ Planned: no key storage, no MLS |
| Operator links peer IDs to IPs | — | Not defended: no relay or proxy is designed |

## Out of scope

- Global passive adversaries correlating traffic across the network.
- Denial of service at the network layer (volumetric floods below libp2p); operators use their
  hosting's protection.
- A compromised operating system on a user's device.
