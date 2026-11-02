# 0034. Desktop tray and single instance

- **Status:** Accepted
- **Date:** 2026-10-29

## Context

[Desktop window and tray](../design/v0.0.1-client-screens.md#desktop-window-and-tray) asks the
Linux and Windows client for a tray icon with Open and Quit, an optional close-to-tray, and a
second launch that brings up the running window instead of a second app on the same profile.
Tauri's tray (the `tray-icon` crate) works on Linux only through a StatusNotifierItem host on the
session bus and the appindicator library, and it neither reports a missing host nor follows one
that goes away, so a window closed to the tray could become unreachable. Tauri has no built-in
single-instance support.

## Decision

- The desktop clients depend on `zbus` (Linux only) to check every two seconds whether
  `org.kde.StatusNotifierWatcher` has an owner on the session bus. Without a host, or without the
  appindicator library, the tray settings are off and closing the window quits; when the host
  goes away the hidden window is shown again.
- A second launch shows and focuses the running window and exits. The Windows client uses
  `tauri-plugin-single-instance`, Tauri's official plugin, with a named mutex. The Linux client
  owns the session bus name `com.dyapp.SingleInstance` itself and serves one `Show` method there
  (`clients/desktop/tray.rs`): the plugin requires zbus 5, whose retired crates the Linux lockfile
  must not carry ([migration](../design/linux-secret-service-migration.md)).
- Both are pinned to exact versions like the other desktop dependencies.

## Consequences

- The window can always be reached: through the tray, a second launch, or the returning window
  when the tray host goes away.
- The Linux client carries about 40 lines for the bus name, covered by the desktop smoke; the
  Windows client adds a small plugin crate maintained by the Tauri team.
- The Docker smoke drives the tray with a fake host (`tests/scripts/fake-tray.py`); real panels
  are checked by hand.
- Single instance is per user session: two users on one machine each get their own app.
