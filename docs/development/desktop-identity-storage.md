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

The file option writes exactly 32 secret bytes to `identity.key` in the app configuration
directory. On Linux it creates the file with mode `0600` and refuses to open files accessible to
other users. On Windows it inherits the app-data directory's ACL; the app does not enforce a
private ACL, so the UI warns that anyone who can read the file can copy the identity. No backend
is selected silently. The non-secret `identity-storage` file in the same directory contains
`system` or `file` and is written before the identity key. An interrupted save can leave the
selection in place without a key; the app then shows an error rather than generating another.
After choosing file storage, the ready screen shows the exact `identity.key` path from the
running app and asks the user to back it up together with `identity-storage` in the same
folder. System storage never shows a file path. The screen does not reveal key bytes.

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
runs before support is claimed.

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
