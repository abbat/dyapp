# Desktop identity runtime lifecycle

The Linux and Windows Tauri shell currently runs Store and network commands against a copied
identity and slot path. [The current ownership audit](../architecture/ffi-design.md#current-desktop-runtime-ownership)
records the gap: switching the selected slot does not wait for those commands, and the verified
profile cache and cancellation flags are process-wide. This page specifies the planned boundary
before the desktop chat adapter adds more long-lived work. The admission/drain primitive has
unit tests; wiring it into Tauri commands and the remaining behavior below are planned.

## Required behavior

- A command holds an active-slot lease from selecting its identity until its Store and network
  work, including its blocking thread, has finished. The lease carries the slot ID, identity,
  directory and generation. New commands cannot acquire the old slot once a switch or removal
  begins. A command must not be force-aborted after it mutates MLS state or SQLite; it finishes
  or cooperatively cancels and reaches a safe commit/rollback boundary.
- Switching marks the runtime as transitioning, cancels the active profile puzzle and network
  check, waits off the UI thread for old leases to finish, stops the embedded node, then opens
  the selected slot. Only after that does it replace the active identity, clear the profile
  cache, and allow new leases. A failed selection either retains the old active slot if no
  persistent selection changed, or shows the existing recovery state; it never silently opens
  an unrelated identity.
- Removal uses the same exclusive transition before deleting any slot files. It verifies that
  the requested slot and confirmed Peer ID still match, drains operations for that slot, stops
  its node, then calls the slot removal path. A failed removal leaves a recoverable state.
- A cache entry or completion belongs to one slot and active network. Clearing the cache after
  the drain is sufficient for the first implementation; later concurrent slots may use a keyed
  cache. Activating a different network invalidates the old verified profile cache too.
- The WebView increments an identity generation when it changes the active identity. Async
  search, profile and Chats callbacks check the generation before changing the page, so a
  response that completed just before the switch cannot paint the new identity's screen.
  Background timers may retry after the transition, but cannot keep the old slot alive.

The switch may wait for a slow request. The UI must show that it is switching and keep the
current identity visible until the transition completes or reports an error. Network operations
need finite deadlines and cancellation checks; the transition must not abandon a blocking
worker that holds a Store transaction.

## Implementation checkpoints

1. Add one runtime gate around all identity-bound Tauri commands, including the network,
   profile, search, privacy, node, usage and chat commands. A lease is held across an async
   call and its blocking child. A transition closes admission before waiting for leases; do
   not hold the gate mutex while awaiting a worker. Use one generation for each active slot.
2. Move the profile-solve and network-check cancellation handles into the active generation;
   cancel and drain them on transition. Keep the MLS `Sessions` mutex to serialize concurrent
   commands *within* one slot. Stop the node only after old client work has drained.
3. Clear the verified profile cache on switch and network activation, and guard WebView result
   rendering by generation. The first implementation may use one cache because only one slot
   is active at a time.
4. Add deterministic tests with paused operations: start a command on A, request a switch to B,
   assert B cannot become active until A commits or rolls back, then assert B sees only B's
   Store and cache. Repeat with removal; a blocked worker must finish before files are removed.
   Test a late A UI completion after B renders, and failure/recovery during selection.

## Choice and trade-offs

| Option | Benefit | Cost or limit |
|---|---|---|
| Admission gate plus drain (chosen) | Covers synchronous, async and blocking commands without moving Store ownership; switching has one observable boundary. | Every identity-bound command must acquire a lease; a slow worker delays switching, so deadlines and cancellation matter. |
| One actor owning every active session | Centralizes ownership and ordering. | Larger rewrite of Tauri commands and harder integration with blocking MLS/SQLite work. |
| Clear only the cache or disable UI buttons | Small local patch. | Background timers and already-running commands can still act on the old slot after a switch. |

This design does not change a wire format or SQLite schema. The shared mobile FFI lifecycle
remains a separate part of the parent client-runtime decision.
