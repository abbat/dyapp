# 0025. Per-profile SQLite client storage

- **Status:** Accepted
- **Date:** 2026-10-24

## Context

The client MLS device and mailbox queue are in memory. A restart loses message history and retry
work, and an interrupted send can roll the MLS state back. The accepted
[delivery decision](0009-message-delivery-and-storage.md)
requires device-held history and a sender retry queue. One local identity may have several devices
and an app may hold several separate profile identities. The existing
[threat decision](../security/threat-model.md#client-side-exposure) accepts platform
data protection and desktop disk encryption for local database confidentiality.

## Decision

Store each profile's local history, settings, processed-envelope IDs and outbound work in one
SQLite database opened at a path supplied by the app. Bind it to the identity's derived Peer ID;
reject a mismatch or unsupported future schema. Use `rusqlite` with bundled SQLite, as the node
already does. Keep the identity and MLS sealing keys in the app's key backend, and only the sealed
MLS state in SQLite. Commit every MLS state change in the same transaction as its inbox or outbox
work before any network send or acknowledgement. Use versioned migrations and a consistent backup
path. The schema and platform path plan are in
[client local storage](../design/client-local-storage.md).

## Consequences

- Transactions let the app retry exact ciphertext and deduplicate inbox items after a crash.
- A profile has one writer and one database to migrate, back up or remove with its local slot.
- Message text and local preferences are plaintext at rest. Platform protections and desktop disk
  encryption remain required; app lock does not encrypt files.
- Bundled SQLite adds native code and build size to client targets. Platform packaging and version
  migration need cross-platform tests before release.
- JSON files would avoid SQLite but cannot atomically pair sealed MLS state with queue entries
  without a custom journal. Whole-database encryption would add key recovery and migration work;
  revisit if the threat model changes.
