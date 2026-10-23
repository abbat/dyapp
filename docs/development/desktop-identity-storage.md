# Desktop identity storage

This page records the planned storage contract for the Linux and Windows Tauri apps. Today
[the shell](../../clients/desktop/main.rs) generates a fresh identity on every launch; it has
no storage picker or system-store integration. The desktop shell calls `dyapp-identity`
directly. The UniFFI `KeyStore` contract serves the native mobile and Apple apps.

## Choice before creation

The app must present storage choices before generating an identity. It may recommend a
backend but must let the user choose. A system-store option is enabled only when its backend
is compiled into the client and a runtime probe of the current user's provider succeeds.
Otherwise the option is disabled with a specific, localized reason, such as no service,
a locked collection, or denied access. No identity-key operation has occurred at that point,
so this is an option state rather than an operation error.

A file option is an explicit user choice with a clear protection warning. Its exact format,
permissions and Windows access-control behavior must be settled and tested before it is
offered. No backend is selected silently. The chosen backend identifier is non-secret
configuration; the identity key stays only in the chosen backend.

Once an identity exists, failure to open the chosen backend must not generate a replacement
identity or silently switch storage. Show the failure and retain the user's recovery options.
Changing backends later requires an explicit, separately tested migration flow.

## Runtime capability contract

A backend reports either `available` or `unavailable(reason)`. The probe checks the provider
used by the running app, never a developer machine or an installed-package list. It does not
create or read an identity key. A successful probe means the provider is reachable now; the
first real save and every later open can still fail, and those failures are reported then.

For Linux Secret Service, the planned probe connects to the user's D-Bus session, finds the
default collection and checks whether it is unlocked. A missing session bus, service or
default collection, and a locked collection, each has a distinct disabled reason. KDE Wallet
and WSL behavior must be tested before claiming support.

For Windows, the planned probe uses the same Credential Manager backend as saving and opening.
The stored credential must use local-user persistence, not enterprise roaming. An API-presence
check alone is insufficient evidence that writes work for every user account.

## Verification before an option ships

- Unit tests cover available, missing, locked and denied probe results and the disabled UI.
- Linux CI starts a controlled Secret Service session and creates and reopens one identity.
- Windows CI uses a real user credential store and creates and reopens one identity.
- A second app launch must show the same peer ID on both platforms.
- A transient backend failure must never create a new peer ID.

The exact crates and storage format need a human-confirmed ADR before new direct dependencies
are added. Until the real backend and restart tests pass, its option remains unavailable.
