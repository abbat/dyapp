# 0022. Optional desktop system credential storage

- **Status:** Accepted
- **Date:** 2026-10-24

## Context

Before this change, the Linux and Windows Tauri shell called `dyapp-identity` directly and
generated a new key on every launch. Its user must choose where a persistent identity key is stored.
A system credential store is only one option; it must not be offered on an account where
the running client cannot use it. The mobile UniFFI `KeyStore` trait is not the desktop
storage API. Losing or replacing the key changes the Peer ID permanently (ADR 0004).

## Decision

Offer system credential storage only after the client probes the provider in the current
user session. The probe does not create an identity key. If the provider is missing,
locked, unsupported or denies access, disable this option with a specific reason. The
user chooses a storage mode before key generation. Save the mode separately from the key.
Never switch modes or generate a replacement identity after a save/open failure.

On Linux use `dbus-secret-service` with its pure-Rust cryptography feature and the Secret
Service default collection through `libdbus`. The probe connects to the session bus, checks
that collection's locked state, then writes, reads and removes an app-owned test item. Store
the 32-byte identity secret as a 64-character hex string so both GNOME Keyring and KDE Wallet
can handle it. Preserve existing `application`/`kind`/`slot` item attributes and the user's
selected storage mode. A provider without a usable default collection is unavailable until a
separately tested path is implemented. The Linux package declares its `libdbus` runtime
requirement; CI builds with the development headers.

On Windows use `keyring-core` 1.0.0 with `windows-native-keyring-store` 1.1.0. Set the
credential's `persistence` modifier to `Local`; the store's default `Enterprise`
persistence could roam. The Windows probe writes, reads and removes a separate app-owned
credential through the same native store API, and checks that its persistence is `Local`.
A successful probe is a current capability signal, not a guarantee that a later write
cannot fail.

The file mode is an explicit user choice with a protection warning, never an automatic
fallback. It stores the raw 32-byte secret in the app configuration directory. Linux creates
it with mode `0600` and rejects looser permissions. Windows inherits that directory's ACL;
the client warns that anyone who can read the file can copy the identity key. The selected
mode is stored separately before creating the key, so a failed save cannot silently start a
new identity on the next launch.

For multiple identities, the choice stays **per identity**. New identities get a random local
UUID slot, independent of their public Peer ID. File keys live at
`profiles/<slot>/identity.key`; system credentials use the same slot in their provider-specific
name. A discoverable profile directory records its mode before key creation. The existing
`identity.key`/`identity-storage` pair and `primary` system credential remain an in-place
legacy slot: adopting them must not move, overwrite or regenerate the key. The local active-slot
pointer is separate from every key. See the [storage layout and recovery rules](../development/desktop-identity-storage.md#multiple-identity-layout-planned).

## Consequences

System storage is optional and may be unavailable on headless Linux, locked sessions or
unsupported Secret Service implementations. Secret Service returns the stored secret to
the application: this is protection at rest, not a non-exportable hardware key.
Both platform backends add direct dependencies and lockfile changes. The real backend
must create and reopen the same Peer ID in Linux and Windows CI before its option ships.
A controlled Linux test must also read an identity item written by the previous
`secret-service` backend without rewriting it. The `dbus-secret-service` migration is
planned; the current Linux binary still uses `secret-service 5.2` until that compatibility
test and the new dependency build pass.
A temporary provider failure must leave the existing identity recoverable and must not
silently create a new one.
The per-identity layout permits independently chosen file and system storage, but its directory
metadata is visible to anyone who can read the app config directory. A system credential is not
included merely by copying that directory; backup/export of system-stored keys needs its own
explicit flow.
