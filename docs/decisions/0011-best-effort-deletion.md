# 0011. Deletion is a signed request, honoured best effort

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

Users need to delete a profile or a message, and GDPR requires erasure. In an open network
([ADR 0007](0007-open-bootstrap-network.md)) nobody can force a third-party node or the other
person's device to delete data.

## Decision

- Profile deletion is a **signed tombstone** from the owner. Nodes should honour it but are not obliged to.
- Bootstrap nodes remove a deleted profile within N days; this is **not guaranteed**.
- Message deletion is an authenticated, encrypted operation to the peer. The peer removes its
  current text and uses its device-signed mailbox `Ack` to ask cooperating nodes to drop known
  envelopes. The sender cannot sign an `Ack` for the peer's mailbox; an offline peer delays this
  best-effort request. Edits replace the current text without a visible history.

## Consequences

- The app must not promise erasure. The UI and privacy policy must say that published or sent data
  may persist on other nodes and devices; legal review of the GDPR position is needed. The
  [message operation contract](../design/message-edits-and-deletion.md) records the chosen
  recipient-mediated request and alternatives; the client implementation is planned.
- Well-behaved nodes need a tombstone store so a deleted profile is not re-imported from peers.
  Tombstones live as long as the node's retention TTL
  ([ADR 0009](0009-message-delivery-and-storage.md)), so a node offline for longer than the TTL can
  serve a deleted profile again.
