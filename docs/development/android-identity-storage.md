# Android identity storage

The Android app keeps several local identities in independent slots, mirroring the
[desktop layout](desktop-identity-storage.md#multiple-identity-layout). Each slot has its own
non-exportable AES-256-GCM key in the Android Keystore that wraps that identity's key file.
No key requires user authentication, so opening or switching an identity never asks for a
PIN or biometrics; an app lock is a separate feature. `Slots.kt` implements this layout and
`SlotsTests` covers it on the JVM with a fake Keystore. The start screen opens the active or
only slot and creates the first one on a fresh install; Switch shows the picker, which lists
each slot by Peer ID or recovery reason and adds identities. A recovery state offers Retry.
Remove opens a confirmation screen for the shown identity (see [Remove](#remove)). The ready
screen also says that uninstalling the app or clearing its data erases the key without warning:
the OS gives the app no chance to warn, and the non-exportable key cannot be backed up yet.

## App-lock record (storage component)

`AppLockStore` persists an app-wide `app-lock` file beside the identity slots in
`noBackupFilesDir`. Its dedicated Android Keystore alias is `dyapp-app-lock`; it does not
change or authenticate the per-slot identity wrapping keys. The 49-byte Rust Argon2id verifier
record and a failure count share one AES-GCM wrapped blob, replaced atomically after each attempt.
The count is persisted before checking a PIN; five failures require an OS device-credential
check. Corrupt or unreadable files fail closed and are never treated as a fresh install.
The UI and OS credential gate are still planned; `AppLockStore` exposes a recovery method that
must only be called after that gate succeeds. `onDevice` uses `PlatformKeyStore` with a dedicated
alias and the common Rust FFI Argon2id functions; JVM tests inject fake implementations.
`AppLockLifecycle` now models fresh-process locking, the 60-second background grace and the
five-minute foreground inactivity deadline from a monotonic clock. `MainActivity` checks it
before constructing `MainScreen`, refreshes on foreground/background and input events, and
schedules its deadline. Configured locks show a private-data-free placeholder until PIN entry
is implemented. The model signals immediate background obscuring; applying that signal to the
app-switcher snapshot remains planned.

## Layout

All files live in `noBackupFilesDir`, which device backup and device-to-device transfer skip
(`android:allowBackup="false"` plus `data_extraction_rules.xml`). A restored blob could not be
opened anyway: Keystore keys never leave the device.

| Item | Location / name |
|---|---|
| Local slot | Random UUID, never derived from the Peer ID or a user label. |
| Lifecycle | `profiles/<slot>/state`: `pending`, `ready`, or `deleting`. |
| Wrapped key | `profiles/<slot>/identity.key`: 12-byte IV, then ciphertext and tag. |
| Wrapping key | Keystore alias `dyapp-identity-wrap:<slot>`, AES-256-GCM, no user authentication. |
| Network settings | `profiles/<slot>/network-draft` and `network`, as on desktop; the legacy slot keeps them at the root. |
| Active selection | `active-profile` contains the slot UUID or `legacy-primary`. |
| Legacy key | Root `identity.key` wrapped by alias `dyapp-identity-wrap`; virtual slot `legacy-primary`. |

There is one storage mode, so a slot has no `storage-mode` marker. The identity list comes
from scanning `profiles/` plus the root legacy file; there is no second index. The app runs
in one process, so a single lock object serializes every slot change; a second process would
need a file lock like the desktop `profiles.lock`.

A per-slot alias means removing an identity deletes its wrapping key, so any leftover copy of
the blob is useless, and a damaged or deleted Keystore entry affects one identity only. A
shared alias would be simpler but could not erase one identity's secret and would lose every
identity at once if that one key were lost.

## Create

1. Pick a fresh UUID and create `profiles/<slot>` with `mkdir`; if the directory or the alias
   already exists, pick a new UUID. Never reuse or delete an existing alias.
2. Write `state` = `pending` (temporary file, `fsync`, rename).
3. Generate the slot's Keystore key, then the identity key; write the wrapped blob to
   `identity.key.tmp`, `fsync` it and rename it to `identity.key`.
4. Read the file back, unwrap it and compare the Peer ID; only then write `state` = `ready`
   and point `active-profile` at the slot.

`active-profile` is written the same way, so a failed write leaves the prior selection. A
missing or corrupt pointer opens the picker, except when exactly one usable slot exists.

## Open and recover

Opening a slot unwraps its file with its alias and derives the Peer ID every time.

| Found on open | Result |
|---|---|
| `ready`, file and alias present, unwrap succeeds | Ready identity. |
| `ready`, file or alias missing | Recovery `missing`; never generate a new key. |
| Keystore or unwrap error | Recovery `unavailable`; the slot and its files stay. |
| `pending` with or without a key | Recovery `incomplete`: show the Peer ID when the key opens; Retry or Remove. |
| `deleting` | Recovery `deleting`: show the Peer ID when the key still opens; Remove finishes. |
| Bad or missing `state`, or a slot path that is not a directory | Recovery `damaged`; Remove refuses it. |

Names under `profiles/` that are not UUIDs are ignored.

Android has no locked-provider case the desktop has: the keys are not bound to user
authentication and `noBackupFilesDir` is credential-encrypted storage, so the app cannot run
before the first unlock. A Keystore failure still shows the `unavailable` state, never a new
identity.

## Remove

Removal follows the desktop rules: the remove screen shows the exact Peer ID, says whether it
is the last local copy, warns that removal cannot be undone only when a key would be lost (not
for a missing key or one another slot also holds), and enables Remove only after the user ticks
the confirmation;
`Slots.remove` then refuses a key whose Peer ID differs from the confirmed one. It writes
`state` = `deleting`, deletes the alias, checks it is gone, deletes `identity.key`, renames the
directory to `profiles/.<slot>.removed` (which drops it from the list) and deletes it. Removal
refuses a folder holding anything but `state` and network settings. A failure keeps the slot
and shows the error; a crash after the rename leaves only the hidden folder. After a crash the
slot shows as `deleting`; cleanup resumes only after the user confirms again or the alias is
confirmed absent. Removing the active slot returns to the picker, which may then hold only Add: no
identity is created on its own. Removal refuses `legacy-primary` and damaged slots, as on
desktop.

## Legacy key

The root `identity.key` and alias `dyapp-identity-wrap` stay readable in place as
`legacy-primary`; nothing moves them. If no active pointer exists, the legacy slot is selected
only when it is the sole slot. A root file whose alias is gone is recovery `missing`. New
identities go only under `profiles/`. An explicit future migration may copy the legacy key to
a UUID slot, verify it and keep the old copy until the user confirms its removal.

## Scenarios to verify

| Scenario | Expected result |
|---|---|
| Fresh install, create, restart | One `ready` slot; same Peer ID after restart. |
| Upgrade with root `identity.key` | `legacy-primary` opens with the same Peer ID; no file moves. |
| Create two identities, switch, restart | Distinct Peer IDs persist; selection survives restart. |
| Crash after `pending` or after the key file is written | `incomplete` recovery; the active slot still opens. |
| UUID directory or alias collision | A new UUID is used; the existing slot is untouched. |
| Alias deleted outside the app | `missing` recovery; no key is regenerated. |
| Remove one of two identities; crash during removal | Only that slot and alias go; a crash leaves `deleting`. |
| Backup and restore to another device | No identities are restored; the app offers setup. |
