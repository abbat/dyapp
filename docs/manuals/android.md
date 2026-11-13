# Android client

Sticker history is prepared to show emoji and pack name without downloading media.
Image display, chat delivery and Add pack remain planned.

Chat photo messages appear in core history order. Incoming previews from people without an open conversation stay blurred until **Show preview**; showing a preview does not download the original. **Download photo** explicitly fetches the original, including on metered networks. Accepted chats also apply the core automatic-download policy and reevaluate it after network changes. Missing originals show an unavailable message; other failures keep retry available. The last-day retention hint is an estimate; a sealed local copy may remain available. In an open chat, **Attach photo…** uses the system picker. The app prepares a bare JPEG and shows its preview for confirmation. Cancel sends nothing; upload refusal retains the prepared photo for retry, while a saved queued send clears it to avoid duplicate delivery. Download-setting controls are still planned.

The Android app requires Android 12 or later. There is no Google developer account or Play Store listing. Users download the release APK directly from [GitHub Releases](https://github.com/abbat/dyapp/releases) and choose whether to install it on their own device. The first app version is still a minimal client; see [platform status](../architecture/overview.md).

The tagged release workflow builds an unsigned APK without secrets, then signs it in a separate GitHub Actions job using the `android-release` environment. It verifies the APK signature, includes the signed `dyapp-X.Y.Z-android.apk` in `SHA256SUMS` and the release provenance attestation, and publishes it only if signing succeeds. Manual workflow runs build but do not publish or sign. No signed APK is available until the release key and CI secrets below are provisioned and the first tag succeeds.

## Signing key setup for the release maintainer

Before the first release, create one long-lived PKCS#12 signing key outside CI with alias `dyapp` and validity of at least 25 years. Make independent offline backups of both the keystore and its password, and test that a backup can sign and verify a disposable APK. Losing this key prevents normal updates of existing installations. Do not commit the key or put it in an issue or log.

Create a protected GitHub Actions environment named `android-release`, restricted to release tags and trusted maintainers. Add its two secrets:

| Secret | Value |
|---|---|
| `ANDROID_RELEASE_KEYSTORE_B64` | Base64 of the complete PKCS#12 keystore file, without line breaks |
| `ANDROID_RELEASE_PASSWORD` | The keystore and key password (the same password for both) |

The signing job needs both secrets. It fails if either is missing, if the keystore cannot sign, or if `apksigner verify` fails. The key is never stored in the repository or passed to the unsigned build job. Keep the offline backup even after the GitHub secrets are set. Before publishing the first tag, verify the environment protection and the key's certificate fingerprint; use the same signing key for every later APK update.

## Background mail

The app schedules a network-constrained background mail job for the active ready identity, requesting
a 15-minute period. Each run retries due queued messages and receives mail through the shared core.
This is not an exact delivery interval or a push wake-up service. The app lock protects the interface;
it does not prevent this background receive. Missing or recovery identities are skipped. The job is
configured as persisted, but delivery under idle restrictions and across reboot or app update still
requires release verification. Foreground use remains available for catch-up.

## Share a profile

On a loaded profile page, **Share profile…** offers existing open chats, excluding the profile owner. Choosing a chat sends a profile link; it never creates a message request. The dialog distinguishes a sent link from one saved locally pending network delivery. After success, **Message** opens the destination chat. Cancel before choosing a chat sends nothing. An exact profile link in a live message appears as a profile button. Press it to load the verified profile; no linked profile is fetched before that press. If unavailable, the profile page offers retry. Back returns to the original chat.

## Install

Download the APK from the intended release and compare it with `SHA256SUMS`. Android may ask you to allow installation from the browser or file manager. On devices subject to [Android developer verification](https://developer.android.com/developer-verification/guides/faq), an unregistered APK may also require the one-time advanced install flow; ADB installation remains available. Requirements can change by region and Android version, so check the current policy before distributing a release. Future updates must be signed with the same key.
