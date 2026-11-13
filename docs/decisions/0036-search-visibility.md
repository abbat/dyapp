# 0036. Search visibility is separate from profile deletion

- **Status:** Accepted
- **Date:** 2026-11-14

## Context

The current Hide from search sends one signed `Profile.deleted` tombstone to both
profile storage and search. It removes the identity's device directory and prevents
KeyPackage lookup, although someone with the Peer ID should still be able to start a chat.
The client cache also treats a profile-store tombstone as final, while the search index
allows a newer ordinary record after a tombstone. ADR 0011 makes deletion best effort;
nodes retain tombstones only for their configured TTL.

## Decision

- Profile storage and search hold separate, owner-signed records. A visible identity has
  an ordinary profile in both. Hide signs a higher-version search-only tombstone
  (`Profile.search_only`, tag 58) in `dyapp/search-withdrawal/v1\0`, sent as
  `SearchRequest.withdraw` (tag 5). Decoding and re-signing preserve the purpose; these bytes
  never authorize ordinary account deletion or device-directory changes. Profile storage
  keeps the last ordinary profile and its certified devices. Direct
  lookup by known Peer ID and KeyPackage lookup continue while hidden. Public profile
  fields remain fetchable by known Peer ID; Hide is not erasure.
- Unhide publishes a still-higher ordinary profile to both services. Search applies its
  existing ten-minute update window; the UI shows pending search publication.
- Identity removal publishes an owner-authorized conditional tombstone to both services.
  A client cache that has seen the profile-store tombstone rejects later versions. Cooperating nodes retain the
  tombstone for their configured TTL. Old, malicious, or expired nodes may still serve
  data: the app never promises network-wide or perpetual removal.
- A failed search withdrawal stays pending and retryable. The UI must not say the
  profile disappeared until a search node accepts the tombstone. Removal retains Retry
  and Remove anyway when a node cannot be reached.
- New ordinary consumers and stores reject legacy unconditional tombstones as account
  authority because old Hide signatures are indistinguishable from account deletion.
  Explicit historical/search verification retains readability without granting authority.
  Legacy pending and acknowledged Hide heads need owner-authorized recovery; migration
  remains planned until implemented and must preserve private state and credentials.
- An unsupported search node leaves withdrawal pending; clients never downgrade to legacy
  publication. Search holders retain new signed withdrawal proofs for authenticated catch-up,
  charge their bytes to storage quotas and preserve them through index generation rebuilding
  without creating visible search rows.

## Alternatives and trade-offs

| Approach | Pros | Cons |
|---|---|---|
| Separate purpose-bound signed records by service (chosen) | A captured Hide cannot authorize account deletion; ordinary device lookup stays available. | New withdrawal requires updated search nodes; unsupported nodes leave it pending, and old signatures require owner recovery. |
| Add `hidden` to Profile | One record per identity. | Old nodes ignore an additive field and may index a hidden profile, or reject a tombstone that carries devices; a coordinated rollout is required to avoid a privacy leak. |
| Allow revival after every tombstone | Small client-cache change. | Weakens final deletion and device-directory rollback protection. |

## Consequences and verification

The client must keep a local pending-withdrawal state and advance versions monotonically
across the two services. Node and client tests cover search withdrawal/unhide, direct
KeyPackage lookup while hidden, final cache behavior, partial failure, and mixed old/new
nodes. The existing best-effort limit in ADR 0011 remains in force.
