# 0028. Closed numeric scales for searchable profile categories

- **Status:** Accepted
- **Date:** 2026-10-25

## Context

Free-text categorical fields split exact search by spelling, case and language. The
education field also could not support “at least a degree”. No client has published
profiles yet (v0.0.1 is unreleased), so these fields and the disposable search index
can change without a compatibility migration.

## Decision

Retire the old strings at tags 10, 11, 13–18 and 25; tag 12 was already retired.
Reserve all those tags. Keep `education_level` at tag 27 with values 0 unpublished,
1 secondary school, 2 vocational or college, 3 bachelor, 4 specialist,
5 master and 6 doctorate. A specialist degree is a single 5–6 year programme
between bachelor and master.

Add the following `uint32` scales. Zero means unpublished for every field.
Values above each maximum are rejected by profile validation.

| Field (tag) | Values in wire order |
|-------------|----------------------|
| `gender` (36) | 1 woman, 2 man, 3 non-binary, 4 other |
| `orientation` (37) | 1 heterosexual, 2 gay/lesbian, 3 bisexual, 4 asexual, 5 other |
| `relationship_goal` (38) | 1 serious relationship, 2 marriage, 3 casual, 4 friendship, 5 not sure yet |
| `role_preference` (39) | 1 breadwinner, 2 equal, 3 homemaker |
| `career_ambition` (40) | 1 low, 2 moderate, 3 high |
| `financial_philosophy` (41) | 1 saver, 2 balanced, 3 spender |
| `employment_status` (42) | 1 full-time, 2 part-time, 3 self-employed/freelance, 4 student, 5 not working, 6 retired |
| `fitness_level` (43) | 1 sedentary, 2 moderate, 3 active, 4 athlete |
| `zodiac` (44) | 1 Aries, 2 Taurus, 3 Gemini, 4 Cancer, 5 Leo, 6 Virgo, 7 Libra, 8 Scorpio, 9 Sagittarius, 10 Capricorn, 11 Aquarius, 12 Pisces |

The single source of values and limits is `rust/profile/src/scales.rs`. Search
nodes index these as integers (zero as NULL). A profile with zero never matches
a value filter. Unordered scales use `in`; career ambition, financial philosophy
and fitness also permit inclusive `range`. The client rechecks node results.
Clients use the core's numeric choices and localize their display names.
Zodiac is self-declared and independent of the birth date
([ADR 0029](0029-partial-public-birth-date.md)).

## Consequences

- Search across clients no longer depends on translated text.
- A new value needs a new number above the current maximum and a wire change;
  existing values cannot be reordered after release.
- Education and the three ordered categorical scales support range queries.
- The pre-release index remains at format version 1; development indexes from
  older prototypes may need a manual reset.
- Free text outside search (profession, company, industry, career stage,
  work-life balance and name) keeps its existing validation and representation.
