# 0036. Search visibility is separate from profile deletion

- **Status:** Accepted
- **Date:** 2026-10-31

## Context

The current Hide from search sends one signed `Profile.deleted` tombstone to both
profile storage and search. It removes the identity's device directory and prevents
KeyPackage lookup, although someone with the Peer ID should still be able to start a chat.
The client cache also treats a profile-store tombstone as final, while the search index
allows a newer ordinary record after a tombstone. ADR 0011 makes deletion best effort;
nodes retain tombstones only for their configured TTL.

## Decision

- Profile storage and search hold separate, owner-signed records. A visible identity has
  an ordinary profile in both. Hide sends a higher-version tombstone to search only;
  profile storage keeps the last ordinary profile and its certified devices. Direct
  lookup by known Peer ID and KeyPackage lookup continue while hidden. Public profile
  fields remain fetchable by known Peer ID; Hide is not erasure.
- Unhide publishes a still-higher ordinary profile to both services. Search applies its
  existing ten-minute update window; the UI shows pending search publication.
- Identity removal publishes a tombstone to both services. A client cache that has seen
  the profile-store tombstone rejects later versions. Cooperating nodes retain the
  tombstone for their configured TTL. Old, malicious, or expired nodes may still serve
  data: the app never promises network-wide or perpetual removal.
- A failed search withdrawal stays pending and retryable. The UI must not say the
  profile disappeared until a search node accepts the tombstone. Removal retains Retry
  and Remove anyway when a node cannot be reached.
- This changes no protobuf field. The signed state is the pair of records held by the
  two services; a tombstone in profile storage, unlike one in search alone, means
  deletion to clients that observe it.

## Alternatives and trade-offs

| Approach | Pros | Cons |
|---|---|---|
| Separate signed records by service (chosen) | Old search nodes accept tombstones and stop indexing; old profile nodes keep serving devices; no wire change. | The two services intentionally hold different versions while hidden; a partially delivered search tombstone leaves discoverability until retry. |
| Add `hidden` to Profile | One record per identity. | Old nodes ignore an additive field and may index a hidden profile, or reject a tombstone that carries devices; a coordinated rollout is required to avoid a privacy leak. |
| Allow revival after every tombstone | Small client-cache change. | Weakens final deletion and device-directory rollback protection. |

## Consequences and verification

The client must keep a local pending-withdrawal state and advance versions monotonically
across the two services. Node and client tests cover search withdrawal/unhide, direct
KeyPackage lookup while hidden, final cache behavior, partial failure, and mixed old/new
nodes. The existing best-effort limit in ADR 0011 remains in force.
