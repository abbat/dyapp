# Linux contact QR camera capture

**Status: design; live capture is not implemented.** The
PNG/JPEG import path and bounded QR validator already exist. A camera scan must
reuse that validator and must never set a verified mark by itself.

## Decision and evidence

Use the WebView's `navigator.mediaDevices.getUserMedia({ video: true,
audio: false })`, attach the stream to a `<video>` with `srcObject`, and sample
into a 768 px grayscale canvas at a bounded rate. Add a Tauri permission
handler that allows **Camera** for the app's `main` WebView only and leaves
other permissions at their default. The capture control is explicit; closing
the panel, changing contact or navigating away stops every track and discards
pending scan results. Show separate localized permission-denied, no-device and
unsupported-runtime outcomes. Keep the existing PNG/JPEG import available.

Why permission handling is required: the [WebKitGTK user-media request](https://webkitgtk.org/reference/webkit2gtk/2.25.1/WebKitUserMediaPermissionRequest.html)
is denied by default when unhandled. The pinned Tauri 2.12.3 includes the
[2.12.0 permission handler](https://tauri.app/release/tauri/all-versions/),
and its locked Wry 0.57.0 routes Linux camera requests through
[WebKitGTK's permission-request signal](https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html).
The [Debian 12 WebKitGTK package](https://packages.debian.org/bookworm/libs/libwebkit2gtk-4.1-0)
currently lists 2.50.6 on amd64; [WebKitGTK 2.50](https://webkitgtk.org/2025/11/26/webkitgtk-2.50.html)
can use the XDG desktop portal for capture devices when available. These are
API and package facts, not proof that a particular machine's camera works.

| Option | Pros | Cons |
| --- | --- | --- |
| WebView capture | Reuses the current canvas, decoder and UI; no new native camera dependency | Needs explicit permission handling; portal, GStreamer and hardware behavior vary by installation |
| Native camera capture | Could isolate WebKit-specific failures | Adds a capture dependency, device and portal integration, and a second pixel transport to maintain |

Choose WebView capture first because it has the smaller implementation and
dependency surface. If a real Debian 12 test shows that a supported camera
cannot produce frames through this path, retain image import and investigate a
native fallback with that failure recorded. Do not infer camera support from an
Xvfb run without a camera.

## Verification checkpoint

The implementation bead must check permission denial, no camera, cancellation,
bounded frame submission, contact switch and a real two-client QR scan on a
Debian 12 computer. Record the host, WebKitGTK package version, portal/session,
camera model, command used and result in the bead. CI can verify the error
states and lifecycle without claiming a real-device pass.
