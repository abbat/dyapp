# 0026. Unicode rules for profile strings

- **Status:** Accepted
- **Date:** 2026-10-26

## Context

Profile strings are public and shown to strangers. Until now only `place` was checked, for control
characters, so bidi overrides, zero-width and private-use characters and non-normalized lookalike
sequences passed ([threat model](../security/threat-model.md#spoofing)). Telling these apart needs
Unicode normalization and general-category data; a hand-written range list misses characters and
falls behind new Unicode versions. `icu_normalizer` and `icu_properties` 2.3 are already in
`Cargo.lock` through `url` and `idna`.

## Decision

`rust/profile` depends directly on `icu_normalizer` and `icu_properties` with compiled data. Every
profile string must be NFC and must not contain control (Cc), format (Cf) or private-use (Co)
characters, except ZWNJ and ZWJ (U+200C, U+200D), which emoji sequences and Persian and Indic
text need. Enum-like strings hold at most 64 code points, `interests` at most 32 items of at most
64 code points each, and `place` keeps its 1024. Interest tags also have one canonical form:
trim surrounding Unicode whitespace, apply Unicode lowercase, then NFC. `rust/profile` exposes
`normalize_interest` and rejects a published interest that differs from that result; client
publication and search inputs must use the helper when interest controls are wired. The node
applies validation through `verify` on publish. Client input and display validation remain
planned.

## Consequences

- One rule, from maintained Unicode tables, guards node and client alike.
- Unassigned code points still pass, so text in newer Unicode is not refused; a newly assigned
  format character can slip through until the crates are updated.
- ZWJ and ZWNJ can still hide inside a string; a lookalike name stays unverified until the
  safety number or QR code is compared.
- The limits are part of the protocol: raising them later lets old nodes refuse new profiles.
