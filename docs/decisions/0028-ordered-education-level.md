# 0028. Ordered education level instead of free-text education

- **Status:** Accepted
- **Date:** 2026-10-25

## Context

`Profile.education` (field 12) was free text, so a search could only ask for exact values and
never for "at least a degree". Search filters are ranges or value lists on node columns
([bootstrap](../architecture/bootstrap.md)). No client has published profiles yet (v0.0.1 is
not released), so the field can change without a migration. The maintainer chose a short scale of
the app's own over free text or an international classification such as ISCED, and asked for the
specialist degree (специалитет) common in Russia and other post-Soviet countries.

## Decision

Field 12 is reserved. `Profile` gains `uint32 education_level = 27`: 0 not published,
1 secondary school, 2 vocational or college, 3 bachelor, 4 specialist, 5 master, 6 doctorate.
A specialist degree is a single 5–6 year programme, so it sits between bachelor and master. A
level above 6 is rejected by `verify`. Search nodes index it as an integer column (index
version 3) and `search_results::Filter::min_education` asks for `Range [min, 6]`.

## Consequences

- "At least" queries are one range condition; an unset level never matches a minimum.
- The scale is coarse: no field of study, and countries map their degrees to it themselves.
- A new level needs a new number above 6 and a wire change; reordering is impossible once
  profiles are published.
- Nodes built before this change ignore field 27 and do not index it; they keep indexing field 12,
  which current clients no longer send.
