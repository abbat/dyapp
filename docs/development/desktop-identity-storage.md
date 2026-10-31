# Desktop identity storage

The Linux and Windows Tauri apps ask where to store an identity key before creating one. They
show its Peer ID after saving it and reopen the same key on restart. The desktop shell calls
`dyapp-identity` directly; the UniFFI `KeyStore` contract serves the native mobile and Apple apps.
The two desktop backends are [system_store.rs](../../clients/desktop/system_store.rs) and
[file_store.rs](../../clients/desktop/file_store.rs).

## Choice before creation

The app presents two choices before generating an identity. It recommends system storage but
lets the user choose. That option is enabled only when its runtime probe in the current user
session succeeds. Otherwise it is disabled with a specific, localized reason. No identity-key
operation has occurred at that point, so the screen shows an unavailable option rather than an
operation error.

The file option writes exactly 32 secret bytes to `identity.key` in a new
`profiles/<slot>/` directory of the app configuration directory. On Linux it creates the file
with mode `0600` and refuses to open files accessible to other users. On Windows it inherits
the app-data directory's ACL; the app does not enforce a private ACL, so the UI warns that
anyone who can read the file can copy the identity. No backend is selected silently. The
non-secret `storage-mode` file in the same directory contains `system` or `file` and is
written before the identity key. An interrupted save leaves an
incomplete slot without a key; the app then reports it rather than generating another.
After choosing file storage, the ready screen shows the exact `identity.key` path from the
running app and asks the user to back up its whole folder. System storage never shows a file
path. The screen does not reveal key bytes.

After selection, failure to open the chosen backend never generates a replacement identity or
switches storage. Restore access to that backend and restart the app. Changing backends later
requires an explicit migration flow, which is not implemented.

## Runtime capability contract

The probe checks the provider used by the running app, never a developer machine or an
installed-package list. It does not create or read an identity key. A successful probe means
the provider is reachable now; the first real save and every later open can still fail.

On Linux, it connects to the user's D-Bus session using a DH-encrypted Secret Service session,
finds the default collection, checks that it is unlocked, then writes, reads and removes an
app-owned test item. Missing session bus, service, default collection and locked collection have
distinct disabled reasons. The key is stored as 64 hex characters so text-only Secret Service
implementations can accept it. KDE Wallet and WSL still need their own real-provider acceptance
runs before support is claimed. A failed Secret Service identity load is logged to stderr;
the picker still shows that slot as unavailable for recovery.

On Windows, it writes, reads and removes a separate app-owned probe credential through the
same Credential Manager backend and checks `Local` persistence. Access denial disables the
option. Identity credentials also use `Local`, not the backend's roaming `Enterprise` default.
A successful probe cannot guarantee that a later write will succeed if account state changes.

## Verification

Linux unit tests use a controlled GNOME Keyring in a D-Bus session; Windows CI tests the real
Credential Manager with a separate test credential and checks `Local` persistence. The desktop
smoke creates and reopens each storage mode in an isolated profile, compares Peer IDs and runs
the pseudo-locale and hidden-screen checks. Provider loss after selection must show an error,
never a new identity. The Windows file backend's inherited ACL and KDE Wallet/WSL providers
remain limits of the current coverage.

[ADR 0022](../decisions/0022-desktop-system-credential-storage.md) records the system-store
choice and its trade-offs.

## Multiple identity layout

The app opens and creates identities through `clients/desktop/slots.rs`: it keeps an older
root `identity.key` / `identity-storage` or `primary` credential readable in place and creates
**new** identities in independent local slots. The window shows a picker (each slot by Peer ID
or recovery reason and storage mode), a recovery screen with Retry, and "Switch or add an
identity" from Settings or the recovery screen. "Remove this identity" in Settings
or on a missing, incomplete or deleting recovery screen shows the Peer ID, storage mode, key file
path and whether it is the last local copy; a no-undo warning appears only when a key would be
lost (not for a missing key or one another slot also holds). Remove stays disabled until a
confirmation box is ticked. The `remove_identity` command then removes the slot as described
below and returns to the picker, or to the storage choice when none is left. It refuses the
legacy slot, which needs the migration below, and a damaged slot, whose files it cannot
interpret. A confirmed removal stops the running node and includes that slot's network and node
settings, the node's key and stores and the `client.db` profile draft (including SQLite
sidecars); unknown files still prevent removal. Merely opening the
profile form does not create `client.db` before the first draft save.

| Item | Planned location / name |
|---|---|
| Local slot | Random UUID, never derived from the public Peer ID or a user label. |
| Lifecycle | `<app-config>/profiles/<slot>/state`: `pending`, `ready`, or `deleting`. |
| File-backed key | `<app-config>/profiles/<slot>/identity.key`, with `storage-mode` = `file` in that directory. |
| Network settings | `<app-config>/profiles/<slot>/network-draft` and `network`, written by `dyapp_identity::network`; the legacy slot keeps them at the root. |
| Node (Linux) | `<app-config>/profiles/<slot>/node.json` holds the node settings and `node/` its key and stores ([ADR 0030](../decisions/0030-desktop-full-node.md)); the legacy slot keeps them at the root. |
| Profile draft database | `<app-config>/profiles/<slot>/client.db`; the virtual legacy slot uses root `client.db`. The broader client store is [planned](../design/client-local-storage.md). |
| Linux system key | Secret Service attributes `application=dyapp`, `kind=identity`, `slot=profile:<slot>`; directory `storage-mode` = `system`. |
| Windows system key | Credential Manager service `com.dyapp.identity`, user `profile:<slot>`, Local persistence; directory `storage-mode` = `system`. |
| Device key | The random device mailbox key, made by the first profile publish and kept in the slot's mode: `<slot dir>/device.key`, or the system key's slot name plus `:device` (`profile:<slot>:device`, legacy `primary:device`). It also derives the key that seals the MLS state in `client.db`; removal erases it before the identity key. |
| Active selection | `<app-config>/active-profile` contains the slot UUID or `legacy-primary`. |
| Legacy key | Root `identity.key` plus `identity-storage`, or existing Linux slot `primary` / Windows user `primary`; virtual slot `legacy-primary`. |

Enumerate `profiles/` and the root legacy marker to show the identity list; there is no
second index that can lose a key after a crash. Reserve a new directory with a fresh UUID
and write its mode and `pending` state **before** saving its key; mark it `ready` only after
the key reopens and yields the expected Peer ID. If the process stops, the reserved slot remains
visible as incomplete. On restart, try to open its chosen backend: if the key exists, derive
and show its Peer ID; if the provider is locked or unavailable, keep the slot and show that
failure; if a usable provider confirms no key, require an explicit retry or removal. Never
generate a second key simply because an incomplete slot was found. Reject symlinked slot
directories and files, malformed slot names and ambiguous duplicate credentials.

`active-profile` is replaced by a flushed temporary file and same-directory rename. A failed
write leaves the prior selection; a missing or corrupt pointer opens the picker, except when
exactly one usable slot exists. Selection does not copy a key or change its backend. Before
switching, the app stops the old identity's network and message state. Read-only display
metadata may cache a Peer ID, but every open derives it again from the actual key and rejects
a mismatch. If a randomly generated UUID collides with a directory or credential, retry with
a new UUID; never overwrite. Concurrent app instances must serialize changes to this directory.
Removal names the exact Peer ID and requires confirmation, especially for the last local copy.
Mark the slot as deleting before removing its file or credential; if removal fails, retain the
slot and show the error. After a crash, resume cleanup only after confirming that the selected
key is absent. Removing the active slot returns to the picker, never switches identities
silently. Local removal does not guarantee erasure of a published profile from the network.

On the first launch after an upgrade, discover the legacy marker and expose `legacy-primary`
without moving the key or credential. If no active pointer exists, select it only when it is
the sole slot. A missing key, locked provider, or root key without a mode marker is a recovery
state, not a reason to create a new identity. Later profile creation writes only under
`profiles/`. A crash before or after writing the active pointer leaves the legacy key intact.
An explicit future migration can move it to a UUID slot only after verifying the new copy and
must keep the old copy until the user confirms removal.

Back up a file-backed new identity by copying its **whole** `profiles/<slot>/` directory,
including `storage-mode`, plus any per-profile network settings. The active pointer is
convenience metadata, not the secret. The legacy file backup remains `identity.key` together
with `identity-storage`. Copying app files alone does not back up system credentials; the UI
must say this and may offer a separately designed export. On restore, inspect the key and
derive its Peer ID before activation. If the incoming slot UUID already exists with a
different key, import under a fresh slot after user confirmation; never overwrite either.
Once the proposed SQLite client store exists, stop the app before a directory copy so its WAL is
checkpointed; a future live export must use SQLite's backup mechanism.

| Scenario to verify on Linux and Windows | Expected result |
|---|---|
| Upgrade file or system `primary`; relaunch with provider available or locked | Same Peer ID or explicit unavailable state; no key move or regeneration. |
| Crash after slot reservation or key save, before activation | Incomplete slot is discoverable and recoverable; existing active slot remains usable. |
| Create two file/system combinations, switch, restart, delete one | Distinct Peer IDs and storage modes persist; only the confirmed slot is removed. |
| Restore a duplicate slot or corrupt the active pointer | No overwrite; picker or collision error preserves both keys. |
| Back up and restore a file slot; copy only app files for a system slot | File Peer ID returns; system slot reports missing credential instead of generating one. |

`tests/scripts/desktop-smoke.py` covers on Linux and Windows: a file legacy upgrade, a pending
slot beside the active one, system and file identities with switch and restart, a corrupt
pointer, a missing file key, and removal of a ready file slot and of a slot whose key is missing
(only that slot folder goes). A locked provider, removal of a system-store key through the UI,
duplicate restore and backup are not covered yet.

This layout favors directory discovery over a single registry: it survives interrupted
registry writes and exposes incomplete slots, at the cost of visible local metadata and a
directory scan. Naming files by Peer ID would simplify lookup but require generating a key
before reserving its slot and reveal public IDs in paths. A single registry would be smaller
but could orphan a successfully saved system credential if its update failed.
