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
`mode` reports the saved choice: `PIN` when a record exists, else `BIOMETRIC` when the empty
`app-lock-biometric` marker exists, else `OFF` after the empty `app-lock-skipped` marker, else
`NEW` for first-launch setup. `skip` writes the skip marker, and clearing the lock also writes it
so setup is not offered again. The app uses either a PIN or biometrics, never both:
`useBiometric` writes its marker before it deletes any PIN record, and a reset writes a new PIN
before it deletes the marker, so a crash mid-switch leaves the app locked with the PIN.
Setup and the settings screen without a PIN offer **Use device biometrics** only when
`canAuthenticate(BIOMETRIC_STRONG)` succeeds, and explain enrolment when none is enrolled. In
biometric mode the app unlocks, and opens its lock settings, only after a `BiometricPrompt`
allowing `BIOMETRIC_STRONG` or `DEVICE_CREDENTIAL`; settings then set a PIN or turn the lock off
without another check.
`resetAfterDeviceCredential` must only be called after a fresh OS check: `UnlockScreen` offers
**Use device screen lock**, a framework `BiometricPrompt` limited to `DEVICE_CREDENTIAL` (no
extra dependency). Its success unlocks the app once it is back in the foreground and opens
`LockSettingsScreen` in reset mode, where a new PIN or Turn off needs no current PIN; identity
slots are untouched. Leaving reset mode keeps the failure count, so a gated PIN still needs the
OS check. Without a device screen lock the button is disabled and the screen explains that it
must be restored. `change` replaces or clears the PIN only when the current PIN,
counted as one attempt, verifies; `MainActivity.setPin` calls it from `LockSettingsScreen`
(and `enroll` when no PIN is set). `onDevice` uses
`PlatformKeyStore` with a dedicated alias and the common Rust FFI Argon2id functions; JVM tests
inject fake implementations.
`AppLockLifecycle` now models fresh-process locking, the 60-second background grace and the
five-minute foreground inactivity deadline from a monotonic clock. `MainActivity` checks it
before constructing `MainScreen`, refreshes on foreground/background and input events, and
schedules its deadline. Before setup is done it shows `SetupScreen` (Skip/Set PIN); a
configured lock shows `UnlockScreen`, which runs the PIN check off the main thread and shows no
private data. While a PIN is set, `MainActivity` keeps the window out of the app-switcher
snapshot from the start, not only after backgrounding: `setRecentsScreenshotEnabled(false)` on
API 33+, and `FLAG_SECURE` on API 31-32, which also blocks screenshots there. Turning the PIN off
lifts it. The app posts no notifications yet; hiding their content while locked is planned with
the first notification.

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

`RustBridge.loadNetwork` and `saveNetworkDraft` pass the ready slot's directory to the shared
FFI, so network drafts and active settings belong to that identity. These blocking calls must
run off the UI thread. The connection form and dial flow are planned; saving these files never
opens or replaces the identity key.

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
also removes the slot's local `profile-draft.json` after confirmation. It refuses a folder
holding anything but its markers, network settings and profile draft. A failure keeps the slot
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
