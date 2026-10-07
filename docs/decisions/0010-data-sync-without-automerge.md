# 0010. Data sync without Automerge: signed profile versions and HLC-ordered messages

- **Status:** Accepted
- **Date:** 2026-10-06

## Context

`rust/crdt-sync` is built on Automerge today. Yet a profile has a single writer (its owner) and is
signed ([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)), and messages are
append-only. `rust/messaging` orders messages by a scalar Lamport clock with no tie-break.

## Decision

- **No Automerge.** A profile is a signed document with a version number; the highest valid
  version wins. Messages are an append-only log per conversation. Revisit Automerge only if data
  with several writers appears.
- **Messages are ordered by a hybrid logical clock** (wall clock + counter), ties broken by Peer ID.
- **Messages sync in real time** (target under 1 s); profiles and the index sync in batches.

## Consequences

- Signed profile versions are implemented (`rust/identity`, `rust/profile`, bootstrap storage) and
  `rust/crdt-sync` (Automerge) is removed. The Lamport ordering in `rust/messaging` must still be
  replaced; dependent docs describe today's code until then.
- A device with a badly wrong clock can reorder its own messages; HLC bounds this but does not
  remove it.
