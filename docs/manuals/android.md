# Android client

Who can see what about you: [Your privacy in dyapp](privacy.md).

Other people's profile pages offer **Save privately** and **Remove from saved**. The mark is stored
for the active identity and shared only with its own devices; it sends no like or notification to
the profile owner. While loading or saving, the action waits for native completion. A read failure
offers Retry before any change. Your own profile has no save action.

In **Search**, **Show saved profiles** opens the private list using local IDs and cached names.
Opening a row then fetches that profile through the ordinary profile page, with its loading and
retry states. **Remove from saved** removes only the private mark and works without fetching the
profile. **Refresh saved profiles** rereads the local list; a failure is shown instead of an empty
list. Pending removal disables row actions, and identity replacement discards the old presentation.

Sticker history is prepared to show emoji and pack name without downloading media.
Image display, chat delivery and Add pack remain planned.

Chat photo messages appear in core history order. Incoming previews from people without an open conversation stay blurred until **Show preview**; showing a preview does not download the original. **Download photo** explicitly fetches the original, including on metered networks. Accepted chats also apply the core automatic-download policy and reevaluate it after network changes. Missing originals show an unavailable message; other failures keep retry available. The last-day retention hint is an estimate; a sealed local copy may remain available. In an open chat, **Attach photo…** uses the system picker. The app prepares a bare JPEG and shows its preview for confirmation. Cancel sends nothing; upload refusal retains the prepared photo for retry, while a saved queued send clears it to avoid duplicate delivery. Download-setting controls are still planned.

The Android app requires Android 12 or later. There is no Google developer account or Play Store listing. Users download the release APK directly from [GitHub Releases](https://github.com/abbat/dyapp/releases) and choose whether to install it on their own device. The first app version is still a minimal client; see [platform status](../architecture/overview.md).

The tagged release workflow builds an unsigned APK without secrets, then signs it in a separate GitHub Actions job using the `android-release` environment. It verifies the APK signature, includes the signed `dyapp-X.Y.Z-android.apk` in `SHA256SUMS` and the release provenance attestation, and publishes it only if signing succeeds. Manual workflow runs build but do not publish or sign. No signed APK is available until the release key and CI secrets below are provisioned and the first tag succeeds.

## Private contact notes

Loaded profiles of other people include a **Private note** editor. It reads and writes only the
active identity's private storage and own-device sync, including for blocked contacts. Notes do not
appear in public profiles, search, likes or chat messages. Loading failure keeps editing disabled
until Retry succeeds; it is not treated as an empty note. Saving waits for native readback. A failed
save retains the exact draft for retry; **Clear private note** clears the saved text only after a
successful native operation. Changing identity or profile opening discards that private draft and
its old presentation continuations. Text validation remains in the common core.

## Release hardening

After signing, the release job runs
`python3 -B tools/scripts/check-hardening.py --android-apk SIGNED.apk`.
Both `arm64-v8a` and `x86_64` must contain the core. Every packaged JNI library,
including dependencies, must be a 64-bit shared ELF of the matching architecture,
with a non-executable stack, full RELRO, no RPATH/RUNPATH or text relocations,
a `__stack_chk_fail` canary import and at least one fortified `__*_chk` libc
import from C code, and no writable executable load. The pinned JNA native rebuild uses
`-D_FORTIFY_SOURCE=2` with `-O2` and `-fstack-protector-strong`; its repackaging step applies
this same ELF policy before an AAR can reach the application. Every load segment must have at least
16 KiB alignment and matching file/virtual page offsets. These imports do not prove that every Rust
or C function is protected.

The same job runs `zipalign -c -P 16 4 SIGNED.apk` from the selected signing
Build-Tools directory, checking packaging alignment independently of ELF alignment.
Failures prevent artifact publication; rebuild the offending library or fix
packaging before signing again. See the [Android compatibility guidance](https://developer.android.com/guide/practices/page-sizes).
These checks do not establish runtime compatibility; device execution remains
required, and a newer NDK alone does not validate third-party prebuilt libraries.

## Signing key setup for the release maintainer

Before the first release, create one long-lived PKCS#12 signing key outside CI with alias `dyapp` and validity of at least 25 years. Make independent offline backups of both the keystore and its password, and test that a backup can sign and verify a disposable APK. Losing this key prevents normal updates of existing installations. Do not commit the key or put it in an issue or log.

Create a protected GitHub Actions environment named `android-release`, restricted to release tags and trusted maintainers. Add its two secrets:

| Secret | Value |
|---|---|
| `ANDROID_RELEASE_KEYSTORE_B64` | Base64 of the complete PKCS#12 keystore file, without line breaks |
| `ANDROID_RELEASE_PASSWORD` | The keystore and key password (the same password for both) |

The signing job needs both secrets. It fails if either is missing, if the keystore cannot sign, or if `apksigner verify` fails. The key is never stored in the repository or passed to the unsigned build job. Keep the offline backup even after the GitHub secrets are set. Before publishing the first tag, verify the environment protection and the key's certificate fingerprint; use the same signing key for every later APK update.

## Message notices

Settings stores message-notice preferences separately for each identity: off, fixed **New message**,
the sender's short ID, or the ID and message text. Turning notices off keeps the content choice.
The switch and content choices wait for each save to complete before accepting another change.
A failed save restores the controls and shows an error. Replacing the identity or leaving the card
cancels its presentation continuation, including any permission request following an old save;
native persistence and message-content policy remain in the shared core.

## Local network permission

On Android 17 or later, the app requests local-network access once after unlocking the interface.
This permission also covers LAN connections made by the Rust core. Denying it leaves public Internet
access available, but LAN nodes can remain unreachable. A manual network check can request access
again; system permission settings remain available after denial. Background jobs never display the
permission dialog. Android 12–16 do not receive this request.

## Background mail

The app schedules a network-constrained background mail job for the active ready identity, requesting
a 15-minute period. Each run retries due queued messages and receives mail through the shared core.
A failed pass asks JobScheduler for a backoff retry instead of waiting for the next periodic run.
This is not an exact delivery interval or a push wake-up service. The app lock protects the interface;
it does not prevent this background receive. Missing or recovery identities are skipped. The job is
configured as persisted; delivery across reboot or app update still requires v0.0.1 verification.
Doze/App Standby can defer jobs and network access until the platform permits them again; this
policy's device verification is planned after v0.0.1. Foreground use remains available for catch-up.

## Share a profile

On a loaded profile page, **Share profile…** offers existing open chats, excluding the profile owner. Choosing a chat sends a profile link; it never creates a message request. The dialog distinguishes a sent link from one saved locally pending network delivery. After success, **Message** opens the destination chat. Cancel before choosing a chat sends nothing. An exact profile link in a live message appears as a profile button. Press it to load the verified profile; no linked profile is fetched before that press. If unavailable, the profile page offers retry. Back returns to the original chat.

## Install

Download the APK from the intended release and compare it with `SHA256SUMS`. Android may ask you to allow installation from the browser or file manager. On devices subject to [Android developer verification](https://developer.android.com/developer-verification/guides/faq), an unregistered APK may also require the one-time advanced install flow; ADB installation remains available. Requirements can change by region and Android version, so check the current policy before distributing a release. Future updates must be signed with the same key.
