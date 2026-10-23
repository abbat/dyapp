# 0022. Optional desktop system credential storage

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

The Linux and Windows Tauri shell currently calls `dyapp-identity` directly and generates a
new key on every launch. Its user must choose where a persistent identity key is stored.
A system credential store is only one option; it must not be offered on an account where
the running client cannot use it. The mobile UniFFI `KeyStore` trait is not the desktop
storage API. Losing or replacing the key changes the Peer ID permanently (ADR 0004).

## Decision

Offer system credential storage only after the client probes the provider in the current
user session. The probe does not create an identity key. If the provider is missing,
locked, unsupported or denies access, disable this option with a specific reason. The
user chooses a storage mode before key generation. Save the mode separately from the key.
Never switch modes or generate a replacement identity after a save/open failure.

On Linux use `secret-service` 5.2.0 with pure-Rust cryptography and the Secret Service
default collection. The probe connects to the session bus and checks that collection's
locked state. Store the 32-byte identity secret as a 64-character hex string so both GNOME
Keyring and KDE Wallet can handle it. A provider without a usable default collection is
unavailable until a separately tested path is implemented.

On Windows use `keyring-core` 1.0.1 with `windows-native-keyring-store` 1.1.0. Set the
credential's `persistence` modifier to `Local`; the store's default `Enterprise`
persistence could roam. The Windows probe uses the same native store API as saving and
opening. A successful probe is a current capability signal, not a guarantee that a
later write cannot fail.

The file mode remains an explicit user choice with a protection warning, not an automatic
fallback. Its on-disk format and Windows access control need their own implementation and
tests before the option is offered.

## Consequences

System storage is optional and may be unavailable on headless Linux, locked sessions or
unsupported Secret Service implementations. Secret Service returns the stored secret to
the application: this is protection at rest, not a non-exportable hardware key.
Both platform backends add direct dependencies and lockfile changes. The real backend
must create and reopen the same Peer ID in Linux and Windows CI before its option ships.
A temporary provider failure must leave the existing identity recoverable and must not
silently create a new one.
