# 0016. Protocol evolution without versions

- **Status:** Accepted
- **Date:** 2026-10-19

## Context

The node protocol has no version numbers ([protobuf-schema.md](../architecture/protobuf-schema.md#evolution-rules-proto3)):
messages change only by new fields and new `oneof` variants. Additive change cannot tighten
security on its own: an old client keeps sending the old, weaker request, and a node that still
answers it keeps the weakness open. `NodeInfo` (`proto/node.proto`) is self-reported by an
untrusted node. Once clients read it, a hostile node could advertise PoW 0, larger limits, a
false clock, or hide a feature so a client falls back to a weaker request. Placement (replica
count, shard layout) that follows what nodes advertise could be steered by a majority of hostile
nodes.

## Decision

- **Capabilities.** `NodeInfo.capabilities` lists optional features a node serves beyond its
  roles. Older nodes send none; clients ignore unknown values.
- **Security tightening is a new `oneof` variant.** A stricter request (for example mailbox write
  tokens) ships as a new variant. Older nodes answer it `UNSUPPORTED`; nodes drop the old variant
  once the minimum supported client sends the new one.
- **`NodeInfo` values are hints only.** A client keeps compiled floors and never goes below them:
  a hint may raise PoW (up to 20 bits) and lower sizes, never the reverse; a hint outside the
  accepted range skips the node (`dyapp_p2p_net::hints`). A client never weakens a
  security-relevant request because a capability is absent once that capability is mandatory,
  and never uses `server_time` for a security decision (at most a median of several nodes for a
  clock-skew warning).
- **Placement is frozen per record.** Replica count and shard layout are protocol constants; a
  change is carried by per-record manifest metadata (for example an additive `shard_size`), with
  readers deployed before writers. Clients never take placement from `NodeInfo`.

## Alternatives and trade-offs

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| Additive fields plus per-operation hints and per-record metadata | Existing clients keep their known operations; each limit or layout has a specific migration path. | Client retries and unknown-field-preserving edits still need implementation; operators must sequence incompatible node limits. | Chosen, with planned behavior called out in the [checklist](../architecture/protobuf-schema.md#evolution-rules-proto3). |
| Versioned protocol IDs and negotiation | A node can explicitly separate incompatible peers. | Every compatible change gains a version rollout and fallback path; it does not itself solve old signed-record retention. | Rejected for these four change classes. |
| Generic capability or attribute map | Fewer schema changes for new fields and limits. | Weakens typed validation and makes old clients' interpretation of new attributes or security requirements ambiguous; untrusted nodes can lie about capabilities. | Rejected as the default; use explicit fields and hints. |

A security-tightening request is the exception: an additive field would let the old weak request
remain valid, so use a new `oneof` variant and retire the old one after clients migrate.

## Consequences

- Nodes and clients upgrade independently; a missing feature shows as `UNSUPPORTED` or an absent
  capability, and the client tries another node.
- A tightening change needs two releases: the new variant first, the old one removed after
  clients move.
- Capabilities and hints cannot be trusted, so they only select nodes and tighten requests; they
  never unlock anything.
- Clients do not read `NodeInfo` yet; the hint check exists and is tested, wiring it in is
  planned.
