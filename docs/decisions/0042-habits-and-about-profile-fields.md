# 0042. Habit scales and about text in the profile

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

Mainstream dating apps offer free text about the person and habits such as smoking and drinking; the
public profile has neither. No client has published profiles yet (v0.0.1 is unreleased), so the wire
and the disposable search index change without a migration.

## Decision

All six fields are optional. Empty text or code 0 means not stated, and clients can clear each one.

| Field | Tag | Values |
| --- | --- | --- |
| `about` | 47 | Text, at most 500 code points |
| `smoking` | 48 | Ordered: no 10, sometimes 20, yes 30 |
| `drinking` | 49 | Ordered: no 10, sometimes 20, yes 30 |
| `tattoos` | 50 | Ordered: none 10, hidden 20, visible 30 |
| `marital_status` | 51 | Never married 1, married 2, divorced 3 |
| `pets` | 52 | No 1, yes 2 |

`about` is plain text: clients show it without links, markup or rich text. It may hold line feeds,
as a few lines read better than one; every line passes the usual text rule, so no other control or
hidden character gets through. Verification on nodes and clients refuses a longer or unclean text.
It is not indexed.

The scales follow [ADR 0028](0028-ordered-education-level.md): nodes take unknown codes, clients
publish only known ones, and search treats a code above the maximum as not stated. A search filter on
any scale leaves out a profile that did not state it. Ordered scales also take ranges.

`pets` is a scale, not an `optional bool` like `has_kids`. Every client builds its pickers, profile
rows and filters from `profile_scales`, so a scale needs no new three-state control. It also leaves
room for kinds of pets later.

A query may hold 32 conditions instead of 16, on nodes and clients. A filter on every field makes 25
conditions and the limit must not refuse what the pickers allow. All conditions must hold inside a
scan that `max_scan_rows` caps, so the extra ones cost little.

## Consequences

- Each client translates the new scale labels; the pickers and filters appear from the scale list.
- The client's recheck and the node index compare the same integer, so they agree on the edges.
- Development indexes from before these columns need a manual reset, as in ADR 0028.
