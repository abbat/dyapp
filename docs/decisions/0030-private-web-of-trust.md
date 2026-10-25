# 0030. Private web of trust from aggregated acquaintance vouches

- **Status:** Proposed
- **Date:** 2026-10-25

## Context

A decentralized dating network has no moderator and no "real person" badge. People still
want three signals about a profile they see:

1. acquaintances of their acquaintances say a real person stands behind it, not a bot;
2. those people consider the person decent, as a subjective opinion;
3. how socially close the profile is: some people look for matches near their own circle,
   others want to avoid it.

A fourth use, weighting complaints by how well the reported person is known, needs separate
research and is not decided here.

The signal must come from "my circle, but I do not know who exactly", never from a named
voucher. No public social graph may be published, and nobody should be able to establish who
vouched for whom. Depth is not capped by a fixed number of hops. Every profile already has a
permanent identity key and peer ID ([ADR 0004](0004-identity-keys.md)), contacts can be
verified by QR code or safety number, and verified pairs already share an MLS group
([ADR 0012](0012-private-p2p-interactions.md)).

A score computable by anyone needs either a public graph, which reveals the vouchers, or common
trust roots, which a decentralized network does not have and bots can imitate. The score is
therefore relative to the viewer, and the graph is never published.

## Decision

- **Participation is voluntary.** A person who wants full anonymity does not verify or vouch,
  and does not appear in anyone's tables. The profile stays public as before.
- **Vouch.** A user may rate a *verified* contact on a local 0–5 scale. 0 means no vouch, 1 means
  "I know this real person", and 2–5 mean increasing decency. The weights are
  `{0, .10, .30, .55, .75, .90}`. A user answers only for the rated person, never for whom that
  person vouches. A rating is suspended while the contact is unverified, for example after it
  adds a device. Negative ratings and warnings are out of scope.
- **Acquaintance edge.** Two users are acquaintances while both rate each other at 1 or more.
  Only acquaintances exchange tables.
- **Aggregated tables.** Each client keeps a table keyed by peer ID. An entry holds a score,
  a coarse distance, and the number of independent sources. The score of a subject is the
  noisy-or of the client's own vouch weight and `δ · s_A` for each acquaintance `A`:
  `s = 1 − (1 − own) · Π (1 − δ · s_A)`. A single acquaintance can add at most `δ`, however
  many profiles stand behind it. This bounds a bot farm attached through one deceived person.
- **Export rules.** Each acquaintance gets its own copy of the table, built under these rules:
  - Split horizon: the copy leaves out that acquaintance's own contribution and its own entry.
  - Two sources: an entry is exported only when it has at least two independent sources.
    A source is the exporter's own vouch or one acquaintance's table. So no recipient can tell
    that the exporter vouched personally.
  - Coarse values: scores are quantized to a few levels. Distance is clamped to at least 2 and
    bucketed, so a direct vouch and a two-hop path look the same.
  - Pruning: entries below the export threshold are dropped, and the table has an entry cap.
  - Depth limit: no hop limit is set. Depth ends where the score fades below the threshold.
- **Viewer display.** The client shows a label on any profile it has an entry for, but only when
  at least two acquaintances contributed. The label states vouching strength (strong,
  noticeable, weak) and distance (close circle, far circle, outside the circle). It never shows
  numbers or vouchers. The copy uses "acquaintances" ("знакомые"), never "friends", "verified" or
  "real". Search offers "acquaintances of acquaintances only" and "hide my close circle" as
  local filters applied to verified results.
- **Transport and freshness.**
  - Tables travel as MLS application messages in the existing verified pair group, padded to
    size buckets.
  - They are sent after a change, at most once a day, and refreshed at least every 7 days.
  - A received table expires after 30 days. It is dropped at once when the edge ends.
  - Revocation is a recomputed table. The network converges within the refresh period because
    each hop multiplies by `δ < 1`.
- **Provisional parameters.** `δ = 0.6`, export threshold 0.05, cap 2000 entries, both two-source
  thresholds 2. These parameters are frozen only after a simulation on synthetic social graphs
  with a Sybil region.
- **Storage.** Vouches and received tables live in the per-profile local store
  ([ADR 0025](0025-client-local-sqlite-storage.md)). Nothing goes to bootstrap nodes except
  encrypted mailbox envelopes.

## Alternatives and trade-offs

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| Aggregated per-viewer tables, noisy-or with per-edge cap | No public graph, unlimited depth, no central roots; a Sybil farm gains at most `δ` per deceived acquaintance; uses existing verified MLS pairs. | Score differs per viewer; acquaintances learn which profiles their circle knows; a modified client sees each acquaintance's aggregate table; cycles inflate scores slightly; the two-source and display thresholds hide sparsely vouched people. | Chosen. |
| Max (best path) instead of noisy-or | Collusion cannot inflate scores. | Ten vouching acquaintances look like one; contradicts the wanted breadth signal. | Rejected; the per-edge cap bounds collusion instead. |
| Public signed vouches (PGP-like graph) | Anyone can compute any score; simple to verify. | Publishes the social graph and every voucher. | Rejected. |
| Direct vouchers only (depth 1, subject-held cards) | Simple; no propagation. | No "acquaintances of acquaintances" signal; signers become linkable when depth grows. | Rejected. |
| Global score from common roots | One number for everyone. | Needs trusted roots or a public graph; bots imitate roots. | Rejected. |
| Differential-privacy noise with fake entries | Formal deniability for presence of a vouch. | Fake entries are false vouches for strangers, including scammers. | Rejected; replaced by the two-source export rule and quantization. |
| Secure aggregation or private set intersection per hop | Even a modified client sees only the total. | Needs acquaintances online together, research-grade cryptography, new dependencies. | Deferred as research. |
| Separate trust in an acquaintance's judgment (PGP owner trust) | Limits a trusting acquaintance. | A second, confusing rating; users answer only for themselves. | Rejected. |
| Propagated "hide me from my circle" flag | Lets a participant hide from near viewers. | Honest-client only; full anonymity is already available by not participating. | Rejected. |

## Consequences

- Nothing is implemented yet. The work adds:
  - a local SQL schema for vouches and received tables;
  - a new chat body kind with a strict binary codec;
  - an aggregation engine, the session send and receive paths, FFI, and UI;
  - a simulation that fixes the parameters.
- A modified client can read each acquaintance's aggregate table and, with very few
  acquaintances, guess the source. The two-source rules reduce but do not remove this risk.
- People vouched by only one acquaintance, or living in sparse clusters, get no label. The cold
  start is slow by design.
- The label is a social signal, not proof of identity, age or safety. Moderation weighting from
  this graph needs its own decision.
- Ratings are opinions about people kept on their authors' devices. A legal review of the shown
  copy is needed before a public release.
