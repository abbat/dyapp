# 0029. Optional partial public birth date

- **Status:** Accepted
- **Date:** 2026-10-25

## Context

The signed profile publishes a self-declared integer `age` (`Profile` tag 3). It
becomes stale without an owner edit, while the search index and client age filter
treat it as current. Some people prefer to publish only a birth year or a year and
month; others do not want to publish any date. A full date identifies a person more
precisely than an age or year.

## Decision

- Add an optional `birth_date` string at the next free `Profile` tag. Its only
  nonempty forms are `YYYY`, `YYYY-MM`, and `YYYY-MM-DD`, with a valid Gregorian
  month/day and four-digit positive year. Empty means unpublished. A month requires
  a year and a day requires a month. No missing part is invented.
- Keep tag 3 as a legacy age snapshot for old signed records. New clients publish
  `age = 0` when using `birth_date`; they never publish both. New readers accept old
  signed profiles and label their integer age as self-declared and possibly stale.
  They cannot infer a birth year from it. Preserve unknown birth-date bytes when an
  older client edits a profile, as with other unknown fields.
- Interpret a partial date as an inclusive interval of possible birth dates:
  a year spans January 1 to December 31, and a year-month spans its first to last
  day. At a given UTC civil date, derive the youngest and oldest possible ages.
  The UI shows an exact age only when both bounds agree, otherwise an age range;
  an absent or future date has unknown age. For a February 29 birthday in a
  non-leap year, use March 1 as the anniversary, conservatively.
- Before publication, the client asks for an explicit 18+ self-attestation even
  when no date is published. If a date is supplied, it also requires the youngest
  possible age to be at least 18. This is a self-declared UI gate, not proof of
  adulthood. Nodes keep rejecting legacy `age` values 1–17 and structurally
  invalid dates; they cannot prove age when the date is absent or false.
- Age search uses a new virtual condition, separate from tag 3, containing the
  lower and upper acceptable birth dates computed for the selected age range and
  query's UTC date. The new search index stores the earliest and latest possible
  birth date for each new profile. A profile matches only when its entire possible
  birth interval lies within those cutoffs; the client reapplies that condition to
  each verified record. New search nodes advertise the virtual field and bump their
  index version. Old nodes skip it, so clients must report partial results; legacy
  age-only records are not counted as current-age matches. Other searches still
  find them. No age is frozen in the index.
- Preview states the chosen precision and warns that a full date is public,
  linkable to other data and may be copied after deletion. It defaults to no
  date; choosing more precision is explicit.

## Alternatives and trade-offs

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| Public partial birth date | Age stays current; users choose precision or none; search can evaluate a possible-age interval. | A full date increases identification risk; new index and query behavior are needed. | Chosen. |
| Keep birth date locally and republish integer age on each birthday | Public profile reveals less; existing age search stays simple. | Offline clients leave stale ages, each identity/device needs scheduling and local date sync, and republishing reveals a birthday transition. | Rejected for the protocol; a local-only date can still be offered separately. |
| Publish birth year only | Simple and less precise than a full date. | Cannot represent a user's explicit month/day choice; age stays uncertain near the birthday. | Rejected as the only form. |

## Consequences

The profile schema, verifier and age-bound helper are implemented; the search index,
client predicate and UI remain planned. Core tests cover old signed bytes, leap dates,
all three precisions and missing dates; search and UI tests still need age-boundary
queries and the conservative 18+ gate. Existing public
age values remain readable, but the protocol does not silently turn them into a
birth date or a guaranteed current age.
