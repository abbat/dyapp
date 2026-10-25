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

- Publish three separate optional `uint32` fields: `birth_year` (tag 33),
  `birth_month` (34), and `birth_day` (35). Each part may be absent independently;
  an explicitly present zero is invalid. Years are 1–9999, months 1–12, and days
  must occur in at least one Gregorian date consistent with the other supplied
  parts. Thus February 29 without a year is valid, but February 29 with a
  non-leap year is not. A day without a month is allowed and is shown as such,
  without guessing a month. An empty set of parts means unpublished.
- Remove the stale `age` field at tag 3 and replace the unreleased string
  `birth_date` field at tag 33. No clients or released signed records need a
  compatibility path. Reserve tag 3 against later accidental reuse. Development
  data written by the old prototype may be reset manually; no protocol or index
  migration is added. The search index format resets to version 1 and stays there
  until the first release.
- With a known year, interpret the supplied parts as the first and last possible
  Gregorian dates in that year, then derive the youngest and oldest possible ages
  at an explicit UTC civil date. A missing month or day is never filled in on the
  profile. A day without a month narrows the possible dates to that day in each
  valid month. The UI shows an exact age only when both bounds agree, otherwise
  an age range. Without a year, or while any possible birth date is in the
  future, age is unknown. For a February 29 birthday in a non-leap year, use
  March 1 as the anniversary, conservatively.
- Before publication, the client asks for an explicit 18+ self-attestation even
  when no year is published. If a year is supplied, it also requires the youngest
  possible age to be at least 18; an incomplete current year has unknown age and
  cannot satisfy this check. This is a self-declared UI gate, not proof of
  adulthood. Nodes reject structurally invalid date parts; they cannot prove
  age when the year is absent or false.
- Age search uses a virtual condition containing the
  lower and upper acceptable birth dates computed for the selected age range and
  query's UTC date. The new search index stores the earliest and latest possible
  birth date only for profiles with a known year. A profile matches only when its
  entire possible birth interval lies within those cutoffs; yearless profiles
  have unknown age and never match. The client reapplies that condition to each
  verified record. Search nodes advertise the virtual field when their index
  supports it. Nodes that skip it make results partial; clients recheck signed
  profiles locally. No age is frozen in the index.
- Preview lists the exact components selected and warns that a full date is
  public, linkable to other data and may be copied after deletion. All three
  fields default to absent; selecting any component is explicit.

## Alternatives and trade-offs

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| Three optional public components | Age stays current when year is known; each disclosure is explicit; month/day works without year. | More combinations to validate; a full date increases identification risk; the unreleased search index must be updated. | Chosen. |
| One partial-date string (including `--MM-DD`) | Smaller wire and parser change. | Encodes independently optional components with special syntax; harder to edit or validate separately. | Replaced by three fields. |
| Separate zodiac only | Reveals less than birthday and already exists as tag 15. | Cannot display or match a chosen birthday; zodiac is self-declared. | Remains an optional independent field. |
| Keep birth date locally and republish integer age on each birthday | Public profile reveals less; existing age search stays simple. | Offline clients leave stale ages, each identity/device needs scheduling and local date sync, and republishing reveals a birthday transition. | Rejected for the protocol; a local-only date can still be offered separately. |
| Publish birth year only | Simple and less precise than a full date. | Cannot represent a user's explicit month/day choice; age stays uncertain near the birthday. | Rejected as the only form. |

## Consequences

The first implementation still uses a tag-33 string, a legacy age field, and
an index built from them. The three optional fields, updated validation and age
helper, adjusted search index, client predicate and UI remain planned. This
unreleased prototype may be changed without a compatibility migration.
