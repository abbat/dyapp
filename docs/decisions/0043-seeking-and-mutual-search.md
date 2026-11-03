# 0043. Who a profile is looking for and mutual search

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

A dating search is most useful when it returns people who are looking for someone like the searcher.
The public profile states neither whom the owner seeks nor that the owner has stopped looking. No
client has published profiles yet (v0.0.1 is unreleased), so the wire and the disposable search index
change without a migration.

## Decision

| Field | Tag | Values |
| --- | --- | --- |
| `seeking_genders` | 53 | `Gender` codes, at most 16, no zero or repeat |
| `seeking_age_from` | 54 | Optional, 18 to 120 |
| `seeking_age_to` | 55 | Optional, 18 to 120, not below `seeking_age_from` |
| `not_looking` | 56 | Bool |

Each field is optional, and clients can clear each one. `not_looking` with any of the other three
invalidates the profile, so the contradiction cannot be published; a client's draft drops the three
when the switch is on. Nodes take unknown gender codes as in
[ADR 0028](0028-ordered-education-level.md), and clients publish only known ones.

A search may ask for mutual matches. The query then carries the searcher's gender and age, and a
result must seek that gender and allow that age. One open age bound leaves that side unlimited. A
profile with no genders or with no age bound is left out, which also leaves out every not-looking
profile. A search without the mutual option treats not-looking profiles like any other.

The check runs on the node, which narrows the results in the index, and again on the client, which
does not trust a node's results.

## Consequences

- A mutual search tells the queried nodes the searcher's exact gender and age. Both are public in the
  searcher's own profile if stated, but the query links them to the session that asked. Filtering on
  the device alone would keep them private, at the cost of scanning results that mostly do not match.
- A client without its owner's gender or full birth date cannot offer the mutual option.
- Development indexes from before these columns need a manual reset, as in ADR 0028.
