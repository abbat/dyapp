# Android client

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

## Install

Download the APK from the intended release and compare it with `SHA256SUMS`. Android may ask you to allow installation from the browser or file manager. On devices subject to [Android developer verification](https://developer.android.com/developer-verification/guides/faq), an unregistered APK may also require the one-time advanced install flow; ADB installation remains available. Requirements can change by region and Android version, so check the current policy before distributing a release. Future updates must be signed with the same key.
