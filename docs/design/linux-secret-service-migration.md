# Linux Secret Service backend migration

Status: proposed, 2026-10-24. The current backend and [ADR 0022](../decisions/0022-desktop-system-credential-storage.md)
remain in force until the maintainer chooses a replacement.

## Problem and invariant

`clients/desktop/system_store.rs` uses `secret-service 5.2` for its Linux
capability probe, `primary` identity slot, and encrypted Secret Service session.
That release still depends on `zbus 5`; zbus 5 depends on the separately retired
`zvariant`, `zbus_names` and `zvariant_derive` crates. A version-only update of
`secret-service` cannot clear the three Scorecard findings. The replacement
must keep the user's system/file choice, not silently switch modes, and must
find existing items by attributes `application=dyapp`, `kind=identity`,
`slot=primary` (later, each profile's slot). Keep the existing hex encoding for
KDE Wallet and the write/read/remove capability probe.

## Options

| Option | Benefit | Cost and risk |
|---|---|---|
| **A. `dbus-secret-service` with `crypto-rust` (recommended)** | Its upstream describes the collection, item and search APIs as a synchronous clone of `secret-service::blocking`; it supports encrypted sessions, so the existing adapter can be migrated with bounded changes. No `zbus 5` dependency. | Uses the C `libdbus` library. The build image needs development headers and the Linux package needs a runtime dependency (or an explicitly tested static build). Changes ADR 0022's pure-Rust choice. Probe/error mapping and old-item compatibility still require controlled tests. |
| **B. Direct `zbus 6` adapter** | Keeps a pure-Rust D-Bus transport and removes the retired standalone crates. | zbus 6 removed its blocking API; we would own Secret Service session setup, DH encryption, collection lookup, search, prompts and error mapping. This is security-sensitive code with a larger test surface. |
| **C. Wait for upstream `secret-service`** | Keeps the accepted API and pure-Rust architecture when its maintainer releases a zbus 6 port. | No released version or schedule exists today; Scorecard remains below baseline. |

Option A is a proposal, not evidence of compatibility. Its upstream
[README](https://github.com/open-source-cooperative/dbus-secret-service)
says the API mirrors `secret-service::blocking`, supports `crypto-rust`, and
requires `libdbus` development headers to build and `libdbus` to run.
The published [`secret-service 5.2` dependencies](https://docs.rs/crate/secret-service/5.2.0)
still name `zbus ^5`; the
[zbus 6 migration guide](https://z-galaxy.github.io/zbus/upgrading-to-6.html)
explains the merged crates and removed blocking API. The similarly named
`zbus-secret-service-keyring-store 1.0.1` still uses `secret-service 5` and
does not clear this dependency path.

## Proposed ADR amendment for option A

Replace ADR 0022's Linux paragraph with:

> On Linux use `dbus-secret-service` with its pure-Rust cryptography feature
> and the Secret Service default collection through `libdbus`. The probe
> connects to the current session bus, checks that collection's locked state,
> then writes, reads and removes an app-owned test item. Store the 32-byte
> identity secret as 64-character hex text so GNOME Keyring and KDE Wallet
> can read it. Preserve existing `application`/`kind`/`slot` item attributes
> and the current user-selected storage mode. The Linux package declares its
> `libdbus` runtime requirement; CI builds with the development headers.

Update its date when accepted. Add an `ADR:` trailer in the dependency-change
commit. Do not apply this paragraph before human confirmation.

## Acceptance checks

1. Resolve `clients/linux/Cargo.lock` and confirm `zvariant`, `zbus_names`,
   `zvariant_derive` and `secret-service` are gone. Run the unchanged audit and
   Scorecard baseline; add no exceptions.
2. In a controlled D-Bus/Secret Service session, load an item created by the
   old backend without rewriting it; verify identical Peer ID. Probe an
   unlocked and a locked collection, unavailable service, access denial and
   absent item. Check duplicate-slot refusal and persistence after restart.
3. Build and smoke-test the Linux package with its declared `libdbus` runtime
   dependency. Verify file mode remains independently selectable.

The migration should be split into an ADR/dependency checkpoint and an adapter
plus smoke-test checkpoint so each can be reviewed and handed off separately.
