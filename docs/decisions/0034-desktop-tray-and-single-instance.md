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
- They depend on `tauri-plugin-single-instance`, Tauri's official plugin: a second launch shows
  and focuses the running window and exits. It uses a D-Bus name on Linux and a named mutex on
  Windows.
- Both are pinned to exact versions like the other desktop dependencies.

## Consequences

- The window can always be reached: through the tray, a second launch, or the returning window
  when the tray host goes away.
- `zbus` is already in the dependency tree through Tauri's Linux stack, so it adds no new code
  owner; the single-instance plugin adds a small crate maintained by the Tauri team.
- The Docker smoke drives the tray with a fake host (`tests/scripts/fake-tray.py`); real panels
  are checked by hand.
- Single instance is per user session: two users on one machine each get their own app.
