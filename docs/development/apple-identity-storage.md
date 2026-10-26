# Apple profile identity slots

## Current boundary

The iOS and macOS start screen now resolves a saved identity without creating
one. A fresh install shows an explicit Create action and a key-loss warning; a
missing or unavailable key shows recovery and Retry. An existing root
`identity.key` remains wrapped by the legacy Keychain item
(`com.dyapp.identity-wrap` / `identity-wrap`) and opens with the same Peer ID.
The shared `IdentitySlotStore` now enumerates UUID slot directories, creates a
`pending` slot with an independent `profile:<lowercase UUID>` Keychain item,
atomically writes its wrapped key, verifies the reopened Peer ID, and marks it
`ready`. Its advisory lock serializes cooperating app processes. A malformed
marker is `damaged`; opening an incomplete or damaged slot never creates a key.
The store also exposes the root identity as `legacy-primary`, reads and atomically
writes `active-profile`, and resolves ready, picker, recovery or first-creation
states without generating a replacement key. An absent root file with its
legacy Keychain item still present is recovery, as is a pointer to that missing
root key. A malformed pointer falls back to the sole slot or the picker without
rewriting the pointer. The runtime uses this resolution for setup, ready and
recovery. Multiple candidates show a picker placeholder until selection controls
are wired; add and switch actions are the next checkpoint.
The Application Support directory is excluded from backup, and Keychain items use
`AfterFirstUnlockThisDeviceOnly`. The profile draft is already named by Peer ID,
so drafts for two identities would not collide.

Desktop and Android already have UUID slots, an `active-profile` pointer,
incomplete and unavailable recovery states, and an explicit remove flow. Their
layouts are described in [desktop identity storage](desktop-identity-storage.md#multiple-identity-layout)
and [Android identity storage](android-identity-storage.md). Apple should use
the same visible states and slot IDs without copying a key between profiles.

## Decision for the Apple implementation

| Item | Planned location or behavior |
|---|---|
| Local slot | Random UUID in `<Application Support>/<bundle ID>/profiles/<slot>/`; never a Peer ID or name. |
| Lifecycle | `state` in the slot directory: `pending`, `ready`, `deleting`; a malformed state is recovery `damaged`. |
| Wrapped key | `identity.key` in that directory, written atomically before `ready`. |
| Wrapping key | Separate Keychain account `profile:<slot>` under service `com.dyapp.identity-wrap`, with the current `AfterFirstUnlockThisDeviceOnly` accessibility. |
| Active selection | Root `active-profile` containing the UUID or `legacy-primary`, replaced atomically. |
| Legacy identity | Existing root `identity.key` and Keychain account `identity-wrap`, exposed as virtual slot `legacy-primary` without moving either. |
| Other local data | Slot directory for future client database and network settings; the current Peer-ID-named profile draft stays separate until the profile runtime is wired. |

The app enumerates the legacy file and UUID directories, not a second registry.
On startup it opens a valid active slot, otherwise the only slot, or shows a
picker when several exist. No slot means a creation screen. Creating a profile
reserves a fresh UUID directory with `pending` first, creates only that slot's
Keychain key and wrapped identity, reopens it to verify the Peer ID, changes the
state to `ready`, then selects it. A collision retries with a new UUID; it never
overwrites an existing directory or Keychain item. Slot mutations are serialized
across app scenes and macOS windows; a second app process must take an advisory
lock around enumeration and changes.

An existing root identity remains readable in place. A missing root key on an
upgrade is a recovery state if an active pointer or other local state refers to
it. On a truly empty installation the new creation screen requires an explicit
Create action. This changes today's automatic first identity creation and
prevents a missing key from silently becoming a different Peer ID. When all
files and Keychain state are gone, the app cannot distinguish a fresh install
from total local data loss; the creation screen must warn about key loss.

Opening derives the Peer ID from the unwrapped identity every time. A missing
file, missing Keychain item, locked Keychain, unreadable blob, malformed marker
or invalid active pointer never creates a replacement. The picker offers Retry
and a clear recovery reason. A corrupt active pointer falls back to the sole
usable slot or the picker, without changing any key. Switching stops the old
identity's future network and message runtime before opening the selected one;
the current shell has no such runtime yet.

Removal is an explicit, confirmed action for a nonlegacy slot. The UI shows the
Peer ID and whether this is its last local copy. Mark `deleting` before removing
the slot's Keychain item; verify that the item is gone, then remove the wrapped
key and slot metadata. If cleanup stops, keep the `deleting` slot visible for
recovery. Refuse removal of a damaged slot and of the legacy slot until a
separate migration flow exists. Removing the active slot returns to the picker;
it never selects another identity silently. Published network data may remain.

## Choice and trade-offs

Per-slot Keychain items make removal independent and allow a missing wrapping
key to affect one profile. They add Keychain operations and more recovery cases.
Reusing today's shared item would be simpler, but all profile blobs would depend
on one item and deleting one profile could not revoke its wrapping key. The
per-slot design is therefore the safer fit for independent identities.

Explicit first creation adds one user step and requires existing first-launch
UI tests to change. It avoids the current `identity()` behavior that generates
a new key whenever the file is absent. Automatic creation with a marker would
keep that interaction, but an old install that lost both its key and marker
would still look fresh. The implementation must never treat an unreadable slot
as fresh.

The current device-only Keychain item and excluded app directory do not form a
portable backup. Copying only `profiles/<slot>/` to another device cannot
restore the identity. A separately designed explicit export and import flow is
needed for portable recovery; the slot implementation must not claim one. It
must not overwrite an existing slot during any future import.

## Verification for implementation

1. Upgrade a root identity and confirm the same Peer ID after selection and
   restart, including a temporarily unavailable Keychain.
2. Create two UUID slots, switch and restart; each retains its own Peer ID and
   profile draft. A pending slot and a corrupt active pointer remain recoverable.
3. Remove one slot after confirmation; the other still opens. Interrupt cleanup
   after `deleting` and check that Retry does not generate a key.
4. Check directory and Keychain account collisions, missing files, malformed
   markers and unwrap errors. No failure path changes another slot.

Shared iOS/macOS unit tests exercise independent per-slot Keychain accounts, cross-account
unwrap failure, the legacy account and an absent account that stays absent after unwrap.
The shared start screen creates and selects a UUID slot only after Create is pressed.
The native iOS/macOS launch tests use isolated slot roots and legacy Keychain
accounts, and check first creation and stable Peer IDs after relaunch. Native CI
validation and picker, selection, removal and recovery fixture tests remain pending.
