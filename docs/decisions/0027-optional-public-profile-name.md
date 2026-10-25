# 0027. Optional public profile name, replaced by local names

- **Status:** Accepted
- **Date:** 2026-10-25

## Context

The profile has no name, so people are told apart by photos and fields alone. A public name is
what a scammer copies first: on its own it would pass for an identity
([threat model](../security/threat-model.md#spoofing)). Profile strings already follow one Unicode
rule ([ADR 0026](0026-profile-text-rules.md)).

## Decision

`Profile` gains an optional `string name = 26`, empty meaning none, held to the enum-like rule: at
most 64 code points, NFC, no control, format or private-use characters except ZWNJ and ZWJ. A
tombstone carries no name. Clients keep a local name per contact, which replaces the public name
everywhere; a public name is always shown with the verified or unverified mark.

## Consequences

- People can name themselves without a central registry; names are not unique.
- Lookalike names remain possible within the text rule; the mark and the safety number or QR code
  are the defence, not the name.
- A node built before this change does not check field 26; current nodes and clients check it
  in `verify`.
