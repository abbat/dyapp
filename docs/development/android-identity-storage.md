# Android identity storage

The Android app keeps several local identities in independent slots, mirroring the
[desktop layout](desktop-identity-storage.md#multiple-identity-layout). Each slot has its own
non-exportable AES-256-GCM key in the Android Keystore that wraps that identity's key file.
No key requires user authentication, so opening or switching an identity never asks for a
PIN or biometrics; an app lock is a separate feature. Chat bootstrap uses
`PlatformKeyStore.existing(alias)`: it can wrap new ancillary secrets only while that
identity's credential exists, and refuses a lost credential instead of recreating its alias.
Explicit identity and app-lock setup retain the credential-creating store. The shared Rust registry implements
this layout; `Slots.kt` projects its results without file or credential lifecycle policy.
Device instrumentation covers real Keystore/native integration, and JVM tests cover recovery
presentation and asynchronous action ownership. The start screen opens the active or
only slot and offers explicit creation on a fresh install; Switch shows the picker, which lists
each slot by Peer ID or recovery reason and adds identities. A recovery state offers Retry.
The profile editor, preview and publication use the active identity's Rust `client.db` draft.
The core validates editable fields before saving; invalid values leave the prior draft intact.
Only a missing core draft starts empty. Key, identity and database failures stop loading instead
of substituting defaults. Obsolete `profile-draft.json` files are ignored and left untouched;
pre-release development data has no migration contract.
The ready screen also opens an editable network draft. It loads this slot's saved draft or active
settings on a background dispatcher, displays the Peer ID and key-loss warning, and saves seed rows
and network name beside that identity's key. A damaged settings file shows an error without
replacing the identity. A ready identity without active network settings opens this draft form
automatically on launch; Back returns to the ready screen for this session. An identity with active
settings returns to the ready screen. The bound factory in `NativeWorkers.kt` constructs a
UniFFI checker for the selected slot on IO; each check uses a fresh cancellation attempt,
and activation also runs off the UI thread. The network form runs a bounded check on IO and
shows an ordered result for each seed row plus a summary. Cancel stops a running check; Retry repeats it, and
Continue activates only a draft that reached a node. Editing clears that checked state, and
failed activation preserves the prior active configuration.
The ready screen opens Settings with a read-only list of local identities, their key storage kind,
and the active marker; Back returns to the same identity. Settings provides explicit Create
and confirmed Remove; switching uses the identity menu. Removing another identity keeps
Settings on the active one, while removing the active identity returns to the picker.
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
lifts it. The new-mail notice carries only fixed text and is private, with a lock-screen version that
shows just the app name ([privacy](../security/privacy.md#local-notices)).

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
| Chat keys | `profiles/<slot>/device.key` (device mailbox key) and `chat-state.key` (MLS state key), wrapped by the same alias, made on the first publish or chat use. Rust refuses a replaced one, so a lost key never resets MLS state; both go with the slot. |
| Network settings | `profiles/<slot>/network-draft` and `network`, as on desktop; the legacy slot keeps them at the root. |
| Active selection | `active-profile` contains the slot UUID or `legacy-primary`. |
| Legacy key | Root `identity.key` wrapped by alias `dyapp-identity-wrap`; virtual slot `legacy-primary`. |

Network reads and saves retain a verified native identity operation for the whole call.
The network form constructs a bound native checker, whose run and activation reacquire core
admission; saving a draft never operates an installation-owned node.

There is one storage mode, so a slot has no `storage-mode` marker. Inventory comes from
scanning `profiles/` plus the root legacy file; there is no second index. The core owns the
cross-process `profiles.lock`: reads/operations retain shared admission, while creation,
selection and removal take exclusive admission. Pending/deleting/broken state, credential
absence and access failures are classified in Rust. Kotlin never validates marker files,
chooses a UUID, generates an identity key or implements cleanup ownership.

A per-slot alias means removing an identity deletes its wrapping key, so any leftover copy of
the blob is useless, and a damaged or deleted Keystore entry affects one identity only. A
shared alias would be simpler but could not erase one identity's secret and would lose every
identity at once if that one key were lost.

## Create

1. The core reserves a fresh UUID directory and refuses existing directories or credentials.
2. It writes `pending` before generating a new identity and asking the platform callback to wrap it
   under a fresh per-slot credential.
3. It durably writes the wrapped blob without replacing an existing file, then reopens it and
   verifies the actual Peer ID.
4. Only after verification does it mark the slot ready and commit the active pointer.

Creation and selection distinguish visible commit from directory persistence. A failed
pre-commit write leaves the previous selection; an unconfirmed post-commit sync returns the
committed identity with a separate diagnostic. Android retains that diagnostic in `Shown`
and logs it as committed. A visible persistence notice remains planned; retrying creation is
never suggested as if the first identity had not been created.

## Open and recover

Opening a slot delegates actual key access and Peer ID verification to the core. Choosing a ready
candidate commits a verified selection. Choosing recovery is navigation only: it leaves the saved
active identity unchanged, and Retry inspects that candidate again. A missing/invalid pointer can
produce recovery without an inventoried location; Kotlin preserves it without fabricating a path
or offering removal. Retry then resolves saved state again.

| Found on open | Result |
|---|---|
| `ready`, file and alias present, unwrap succeeds | Ready identity. |
| `ready`, file or alias missing | Recovery `missing`; never generate a new key. |
| Keystore or unwrap error | Recovery `unavailable`; the slot and its files stay. |
| `pending` with or without a key | Recovery `incomplete`: show the Peer ID when the key opens; Retry or Remove. |
| `deleting` | Recovery `deleting`: show the Peer ID when the key still opens; Remove finishes. |
| Bad or missing `state`, or a slot path that is not a directory | Recovery `damaged`; Remove refuses it. |
| Missing/invalid pointer without an inventoried candidate | Recovery without a location; no fabricated slot or Remove action. |

Names under `profiles/` that are not UUIDs are ignored.

Android has no locked-provider case the desktop has: the keys are not bound to user
authentication and `noBackupFilesDir` is credential-encrypted storage, so the app cannot run
before the first unlock. A Keystore failure still shows the `unavailable` state, never a new
identity.

## Worker ownership

`RustBridge.withKey` retains the native operation through the entire blocking action; callers
must not return a worker that will use its directory/key after that scope ends. Short-lived
publisher hide/withdraw calls remain inside this scope. Persistent proof, search and network
handles use the bound factories in `NativeWorkers.kt`; every actual native call reopens and
verifies its owner. Search and contact-name reads use keyless native methods, without an outer
`withKey` lease. Idle handles do not prevent selection/removal, and a stale handle cannot
recreate a removed slot or use a replacement key. Cached photo reads remain memory-only.

Chat bootstrap and ancillary-key persistence belong to the core. `withChats` directly opens a
registry-owned `ChatSession`, whose identity and chat locks last through actual completion.
It uses only the slot's existing wrapping credential. Kotlin does not generate or write
`device.key` or `chat-state.key`, and initialized chats never regenerate a missing key.

Root identity actions run on IO. The existing route shows loading until completion, rejects
duplicate actions and never publishes a cancelled screen owner's late result. Its completed revision
advances after each actual result, even when the visible screen stays the same; test semantics
report zero while pending. Instrumentation awaits this completion signal instead of an intermediate
loading frame or an old matching Recovery view. Identity-menu and Settings inventories, the
profile editor's initial draft/visibility snapshot and the publication preview also load on IO.
Their keyed presentation state starts empty for a replacement owner and rejects cancelled
owners' late values. The editor and preview show loading; publication remains disabled until
the preview has loaded. Removal preflight and editor persistence also run on IO.
Save freezes the typed input and field maps before dispatch, rejects duplicate commands and
disables publication while pending. A completed write marks only its submitted snapshot saved.
If inputs changed meanwhile, it preserves them; otherwise it shows the core-normalized seeking
values and records that normalized presentation as the saved baseline. Cancellation suppresses
the old owner's result even when the native write finishes. Save test semantics advance a
completed revision per result and report zero while pending, so an identical old status text
cannot satisfy a new command. The value-read helper must not return native handles.
Network checkers, proof solvers and the search session use `withNativeResource`: creation and final release
stay on IO, while presentation callbacks return to the caller's context. If cancellation
arrives during a blocking factory, its eventual handle is closed without being published.
The network screen retains its checker in this scope until disposal; teardown cancels it
before IO release, without a competing UI-side close. Search requests await the owned
searcher's readiness, and verified-photo providers refresh when it is ready. Disposing
the search session cancels its owner without waiting for construction or closing on UI.
The common native API now offers a single-use `ConnectionAttempt`, backed by a core
cancellation owner that exists before dispatch and survives pending admission. Each retry
creates a fresh owner. Android prepares this owner before publishing Checking and dispatching
IO. Cancel and screen disposal signal that current attempt immediately, including during key
admission. A late native result cannot become Connected after Cancel. Actual completion clears
only its owner and releases the native handle on non-cancellable IO; retries allocate a new handle.

While process-local draft serialization remains, every `draftWrites` monitor is acquired before
native admission, including publication. A lease holder never waits for that monitor. Shared
core serialization of draft transformations across independent processes/clients remains planned;
registry admission protects identity lifetime and is not itself a draft-write mutex.

## Remove

The confirmation first loads the target-owned copy/visibility snapshot on IO. Loading and
read failure leave confirmation unavailable; Cancel remains available. A visibility read error
never counts as an unpublished profile. Once loaded, it shows the exact Peer ID and whether
another ready slot has that key. Changing targets resets both loading and confirmation.
Removal requires confirmation and delegates identity/key verification, owned-file preflight,
credential erasure and cleanup to the core. Unknown files, symlinks or unowned directories refuse
removal before any credential is erased. A key that still opens must match the confirmed Peer ID.

The core marks interrupted removal as deleting, clears a selection targeting that identity,
erases and verifies absence of its wrapping credential, and removes only validated identity data.
This includes chat keys/lock, profile and chat database side files, network settings and validated
media directories. Installation-owned node and app-lock data are outside this cleanup. A failure
retains discoverable recovery state; old hidden `.<slot>.removed` quarantine directories also
remain visible for explicit recovery. No client-generated path or recursive unknown-file deletion
participates in normal removal. Legacy and damaged candidates remain unavailable for removal.

Exclusive removal waits for real admitted workers to finish; the UI dispatches that wait off
its main thread. Visible completion and any persistence diagnostic remain distinct. Removing the
shown identity returns to the picker and never creates another identity automatically.

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
