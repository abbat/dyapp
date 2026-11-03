# 0041. Optional profile height

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

People filter on height in mainstream dating apps, and the public profile has no such field.
Weight stays out: `fitness_level` already covers build. No client has published profiles yet
(v0.0.1 is unreleased), so the wire and the disposable search index change without a migration.

## Decision

`height` is `optional uint32`, tag 46, in whole centimetres, valid from 50 to 272 inclusive.
Unset is absent on the wire, not zero. Centimetres are the only unit on the wire and in the index.

Unlike the closed scales ([ADR 0028](0028-ordered-education-level.md)), verification on nodes and
clients refuses a value outside the range. The range is a physical limit, not a list that a later
release extends, so there is no newer code for an old node to tolerate.

The search filter takes a lower bound, an upper bound or both, each inclusive. A missing bound
becomes the edge of the valid range, so the wire condition is always `range` on field 46. A reversed
range is refused by the client's filter and is `INVALID` on the node. Any bound leaves out profiles
without height, since the searcher asked about height; with no bound height has no effect.

## Consequences

- The client's recheck and the node index compare the same integer, so they agree on the edges.
- Widening the range later needs old nodes to be gone first, as they would refuse such a profile.
- Development indexes from before this column need a manual reset, as in ADR 0028.
