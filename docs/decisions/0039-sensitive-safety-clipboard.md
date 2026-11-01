# 0039. Sensitive safety-number clipboard on desktop

- **Status:** Accepted
- **Date:** 2026-10-31

## Context

The desktop safety panel offers an explicit copy of the 60-digit comparison
number. It should try to clear that copy after 60 seconds without erasing a
later clipboard value. The existing Web Clipboard API has no sensitive-content
hint. [WebKitGTK denies unhandled clipboard-read requests](https://webkitgtk.org/reference/webkit2gtk/2.41.3/class.ClipboardPermissionRequest.html),
and locked Wry 0.57 does not route Linux clipboard reads through its
[permission handler](https://docs.rs/wry/0.57.0/wry/enum.PermissionKind.html).
Browser-only expiry would therefore be unreliable on Linux.

## Decision

Add the official `tauri-plugin-clipboard-manager` 2.5.0 to Linux and Windows.
An app command recomputes the safety number from the active identity and chosen
Peer ID, then uses the plugin's `write_secret` to request clipboard-history
exclusion where supported. The WebView starts a 60-second timer; at expiry an
app command reads and compares the current clipboard off the GUI thread and
clears it only if unchanged. A generation token prevents an older timer from
clearing a later in-app copy. The plugin's general JavaScript commands are not
granted to the WebView. Ordinary Peer ID copy keeps its current behavior.

## Consequences

The plugin maintains clipboard ownership and supplies a native read path, but
adds a direct dependency and transitive build cost. History exclusion is a
hint, not a guarantee. If clipboard access fails, the app leaves the value
untouched. An external copy of identical bytes, or a change between read and
clear, cannot be distinguished without an OS change token. The timer needs a
running WebView; suspension may delay expiry. Linux and Windows CI check the
native command and a fake 60-second timer; a real desktop-session check is
still required for the platform's clipboard history behavior.
