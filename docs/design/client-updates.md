# Client update discovery and notifications

Status: design complete; implementation and live publisher verification are pending.
No client currently checks for updates. The current release workflow publishes `dyappd` only.

## Goal and boundary

For v0.0.1, notify a client when a newer release of that *client platform* is
available. The notice opens an official release destination; it never downloads,
installs or executes an update. A new `dyappd` tag alone is not a client update.
This page specifies the stable channel first. Prerelease opt-in needs a separate
contract and cannot silently enter the stable feed.

## Chosen source

A release job will publish a small static JSON feed through GitHub Pages at the
proposed URL `https://abbat.github.io/dyapp/updates/v1/stable.json`. This URL is
**not live yet**. Enable Pages and verify the live URL before turning on client
checks. The job must run after release assets are uploaded and their provenance
attestations are available. It derives entries from published `vX.Y.Z` GitHub
Releases and records a platform only after its installable artifact, store
listing, or tested source-build path has been verified. Missing platforms keep
their last verified version; they never inherit the newest server or another
platform's version.
A publisher smoke test must check the feed over HTTPS after deployment.

The app accepts HTTPS from the exact `abbat.github.io` host and fixed project
path, with no cross-origin redirects, and limits response size (64 KiB). It
validates a versioned schema before use. A minimal shape is:

```json
{
  "schema": 1,
  "channel": "stable",
  "targets": {
    "linux-x86_64": {"version": "0.0.1", "tag": "v0.0.1", "method": "package"}
  }
}
```

`targets` uses `linux-x86_64`, `windows-x86_64`, `macos-universal`, `android`
and `ios` for the first release. `method` is `package`, `store`, or
`build-from-source`; unknown methods fail closed. The example entry does not
claim a Linux client release already exists. Feed data
contains no arbitrary URL, release-note HTML, script or executable command.

### Release candidate adapter checkpoint

`tools/scripts/release-candidates.py RELEASE.json TAG COMMIT` parses the bounded
GitHub REST get-by-tag response into exact Linux and Windows package candidates.
It rejects draft/prerelease, unpublished or mismatched tags, duplicate or wrong-
version client assets, missing SHA-256 digests and invalid package sizes. The
commit argument must come from a separately resolved, checked-out tag; GitHub's
`target_commitish` can be a branch and is not tag proof. Candidate output has no
trusted `releases` or `targets` fields and cannot be passed to the feed generator
as verification evidence. Download, digest and attestation checks are the next
checkpoint before constructing that trusted inventory.

### Offline feed generator checkpoint

`tools/scripts/build-update-feed.py` turns a CI-verified inventory and optional
prior feed into schema 1 stable JSON on stdout. It checks structure, not
signatures or Pages. Release CI must verify published tags, asset digests,
attestations and source builds, fetch the prior feed from its fixed origin,
then deploy and smoke-test Pages. Client checks stay off until then.

The inventory has `schema: 1` and bounded `releases`. Each release has `tag`,
40-hex `commit`, `draft`, `prerelease`, `assets`, `attestations`, `source_builds`
and `targets`. An asset has `name`, `state: "uploaded"`, `sha256`; an attestation
repeats its name and digest. A source build has `target` and the release
`commit`. A target claim has `method` and, for `package`, an `asset`. Linux
requires `dyapp_<version>_amd64.deb`; Windows requires
`dyapp-<version>-x86_64-windows.zip`, uploaded with matching attestation.
`build-from-source` accepts only macOS and iOS with same-commit source-build
records. These records are CI assertions, not proof supplied by the JSON.
`store`, Android packages and unsigned Apple packages fail closed pending an
installable artifact or pinned store destination policy.

Versions are compared numerically; server-only tags preserve platform entries.
Draft/prerelease client claims, duplicate tags, missing or failed assets,
absent attestations, invalid versions, same-version rewrites and rollback fail
generation. A malformed prior feed is rejected, never silently reset.

The client constructs the release page from the fixed repository and validated
tag: `https://github.com/abbat/dyapp/releases/tag/vX.Y.Z`. A store destination,
when applicable, is separately pinned in the app and enabled only after its
listing is verified. The UI must describe the notice as release availability,
not as proof that an installer is safe.

## Version and rollback rules

- Stable tags are exactly `vMAJOR.MINOR.PATCH` with decimal nonnegative
  components and no leading zeros except `0`. Parse components as bounded
  integers; never compare version strings lexically or as floating-point values.
  `tag` must equal `v` plus `version`. Unknown schema or channel fails closed;
  unknown target IDs are ignored so a later platform does not break older apps.
- Show a notice only when the target version is strictly greater than the
  installed app version and compatible with that app/platform. Missing target,
  equal or older version means no new notice. Protocol compatibility requirements
  must be explicit in feed schema before the publisher uses them; clients must
  reject an unknown requirement rather than guess.
- Remember the highest *validated* version observed for this target. A later
  lower feed entry is treated as rollback or stale publication and does not
  generate a notice. This is a local guard, not publisher authentication.
  Fix a mistaken release by publishing a higher version, never replacing a tag
  or reusing a version (the existing release ADR already requires this).
- A release gate checks that each advertised tag exists in `abbat/dyapp`, is
  published and stable, names the expected client artifact or verified store
  listing (or a tested source-build path), and advances only the targets
  actually shipped. A failed gate leaves the previous feed intact and fails
  the release. The publisher also verifies
  artifact provenance where available; the operator must still verify a
  downloaded package as described in the release manual.

## Trust and failure model

HTTPS authenticates the hosting origin and protects the feed in transit. The
publisher runs in this repository's release pipeline with a protected Pages
deployment environment and narrowly scoped `pages:write` / `id-token:write`
permissions. This does not create a signing authority independent of the GitHub
account: a compromised repository or Pages deployment can lie about a version.
The fixed release URL, strict data-only schema, no automatic installation and
provenance instructions limit what such a lie can do. A malicious high version
can still suppress later notices on a device; recovery needs an explicit
operator-visible reset of the cached highest version after investigating the
publisher. No feed error may silently install anything.

Network errors, invalid JSON, unexpected redirects, missing platform entries,
rollback, and publisher failures produce no new-version notice. They must remain
visible to a manual check and in support diagnostics without exposing identity
keys or bootstrap/network configuration. The update request is independent
of the active identity and should carry no
Peer ID or user profile data.

## Check cadence and privacy

The app-level setting is shared by all identities on one installation. Offer
**6 hours**, **12 hours**, **24 hours (default)** and **7 days**, plus **Check
now**. A manual check does not change the selected interval. The default means
an attempted check no later than 24 hours after the last successful check
*while the app can run and network is available*, not a promise to wake a
suspended or closed app at an exact wall-clock time.

Persist `last_attempt`, `last_success`, the selected interval, the validated
feed and the highest seen version per target. A valid 200 response or a 304
with a previously validated cached feed counts as success; an invalid response
does not. On first launch or foreground resume, check immediately if no success
exists or the interval elapsed. While running, schedule the next attempt
between 23 and 24 hours after success for the default setting (proportionally
before the deadline for other intervals), with per-installation jitter to avoid
a common request spike. Coalesce simultaneous triggers into one request.
After a failure, retry with bounded exponential backoff from 15 minutes to 6
hours while the app is runnable; also try on network restoration, respecting a
15-minute minimum since the last attempt. A manual check can bypass that
minimum unless a request is already in flight. Do not spin on invalid metadata
or rate-limit responses. Persist wall-clock timestamps and clamp future times
after clock changes; use a monotonic clock for timers during one process run.

Use one HTTPS `GET` to the fixed feed path. Send no Peer ID, account token,
profile, bootstrap list, network name or per-installation identifier, including
in query parameters. The hosting provider still sees the device IP and normal
HTTP metadata. Do not add analytics to this check. Offline/error state never
blocks app startup or changes a stored identity.

| Platform | While running | Background or closed | Catch-up |
|---|---|---|---|
| Linux, Windows, macOS | In-process timer while the app remains open or in a working tray/menu-bar state. | A closed process does not check; no new daemon or login item is installed. Sleep can delay a timer. | On launch, resume and wake. |
| Android | Foreground timer/lifecycle check. | WorkManager periodic work with network constraint is planned; Doze and battery policy can delay it. Its new direct dependency needs an ADR before implementation. | On foreground resume and connectivity restoration. |
| iOS | Foreground lifecycle check. | `BGAppRefreshTask` is best effort when permitted; `earliestBeginDate` is not a deadline. Disabled Background App Refresh, suspension or force quit can prevent a run. | On next foreground launch/resume. |

No mobile platform can guarantee a completed check in every 24-hour wall-clock
window while dormant. A hard deadline would need a separate push service and
still depends on OS delivery; it is outside v0.0.1. The UI must explain the
last successful check and that the next background check is approximate.
[Android WorkManager](https://developer.android.com/reference/androidx/work/PeriodicWorkRequest)
and [Apple BackgroundTasks](https://developer.apple.com/documentation/backgroundtasks/bgtaskrequest/earliestbegindate)
document these scheduling limits.

## Platform delivery

The notice destination is pinned by the installed platform and `method`, never
taken as an arbitrary URL from the feed. The publisher may advertise
`build-from-source` only after CI built and tested that platform at the tag and
the platform manual has usable signing/install steps. It must not call an
unsigned build an installable update.

| Target | Current publication evidence | First valid destination |
|---|---|---|
| `linux-x86_64` | `make deb` builds client `.deb` and tarball, but `release.yml` does not upload them. | `package` only after the exact client assets and provenance join GitHub Releases; open the fixed release page and show verification instructions. |
| `windows-x86_64` | `release-sizes` produces a portable unsigned ZIP as a workflow artifact, not a release asset. | `package` after that ZIP and checksum/provenance join GitHub Releases; warn that code signing is still pending. |
| `macos-universal` | CI produces an unsigned, unnotarized ZIP; there is no Developer ID. | `build-from-source` with the macOS manual until a signed/notarized distribution exists; never imply the ZIP opens as downloaded. |
| `android` | App ID is `com.dyapp`; release signing and a store listing are pending. | `package` for a verified, consistently signed APK, `store` only after a real listing, or `build-from-source` with explicit non-upgrade caveat for developer builds. |
| `ios` | The CI IPA is unsigned and cannot be installed as published; no Apple Developer Program account exists. | `build-from-source` with the iOS self-signing manual. `store` requires an actual approved listing. |

The Android publisher must also check a matching application ID, compatible
signing certificate and increasing `versionCode`; otherwise installing the
candidate can fail or require uninstalling the existing app, which loses data.
[Android's update rules](https://developer.android.com/google/play/app-updates)
specify these conditions. The current Apple manuals describe the unsigned
builds; the Windows manual describes its portable ZIP. Platform-specific
publication and signing are existing release tasks; this design does not mark
them done.

## Why this source

| Option | Pros | Cons |
|---|---|---|
| Static Pages feed (chosen) | One cacheable request without a token; explicit per-platform availability; no GitHub REST quota shared by users behind one IP | Needs Pages setup and release publisher; stale feed if publishing fails; same GitHub account remains the trust root |
| GitHub Releases REST API | Existing release data, no separate publisher | Unauthenticated requests share a 60/hour/IP quota; latest-release ordering is not numeric version ordering; platform filtering and pagination complicate each client |
| App-store-only notices | Stores own listing and delivery on mobile | Does not cover Linux/Windows and cannot provide one version contract across clients |

The current [`release.yml`](../../.github/workflows/release.yml) collects only
`dyappd` files despite `make deb` also building Linux client packages.
[ADR 0020](../decisions/0020-dyappd-release-channels.md) plans client artifacts on
the shared release tag; publishing them and the feed are implementation work.
GitHub documents [Pages workflow permissions](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages),
[release API ordering](https://docs.github.com/en/rest/releases/releases) and
[REST rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api).

## Settings and notification UX

Settings exposes the four intervals above, **Check now**, installed version,
last successful check (local date/time or **Never**), and the current update
state. A failed check must not replace the last known valid version. Changing
the interval persists immediately and recomputes the next due check; shortening
it past due schedules a check, lengthening it does not force one. The setting
and dismissal state belong to the installation, not to an identity. While the
feed or platform destination is unfinished, controls remain disabled with a
specific explanation, as required by the [screen draft](v0.0.1-client-screens.md).

On a newly discovered higher version, show one in-app notice with installed and
available versions. While background execution succeeds, the app may also
request one OS notification for that version. Where permission is needed, ask
for it from a foreground Settings action before background notification use;
never prompt from a background task. If permission is unavailable or denied,
the in-app notice and Settings badge still work on next open. No permission is
required just to check. **Later** dismisses the interruption for that version,
not the Settings badge; a newer version can notify again. Do not repeatedly
show an OS notification for the same version. An installed version at least as
new as the notice clears it and the badge. No release notes or URL from feed
JSON are rendered as markup.

| State code | English UI | Russian UI | Action |
|---|---|---|---|
| `checking` | Checking for updates… | Проверка обновлений… | Coalesce repeated taps; keep Cancel/Back if the platform needs it. |
| `current` | Version {installed} is up to date. | Версия {installed} актуальна. | Show only after a verified feed check for this target. |
| `available-package` | Version {available} is available. Open the official release page. | Доступна версия {available}. Откройте официальную страницу релиза. | Open the fixed GitHub release page; explain package verification. |
| `available-store` | Version {available} is available in the app store. | Версия {available} доступна в магазине приложений. | Open the app-pinned store listing. |
| `available-source` | Source for version {available} is available. Build and sign it using the platform guide. | Доступны исходники версии {available}. Соберите и подпишите приложение по инструкции для вашей платформы. | Open the fixed tag and relevant manual; never present an unsigned IPA as installable. |
| `offline` | No connection. Last checked: {date/never}. | Нет соединения. Последняя проверка: {дата/никогда}. | Retry manually or wait for backoff/network restoration. |
| `untrusted` | Update information could not be verified. Try again later. | Не удалось проверить сведения об обновлениях. Попробуйте позже. | Keep prior validated state; do not open a feed-supplied link. |
| `not-published` | Updates for this platform are not published yet. | Обновления для этой платформы пока не опубликованы. | Never claim the installed version is latest. |
| `background-limited` | Background checks may run late; updates are checked when you open the app. | Фоновая проверка может задерживаться; при открытии приложение проверит обновления. | Show as an explanation when platform background work is unavailable. |

The notice controls are **Open official release / Открыть официальный релиз**
(or **Open store / Открыть магазин**, **Build instructions / Инструкция по
сборке**) and **Later / Позже**. Display release notes only on the fixed
external page, not as untrusted HTML in the app. Background failures do not
raise repetitive dialogs or OS alerts; manual checks explain the failure.

## Acceptance matrix

- A valid higher version for the installed target yields one localized notice
  and a persistent Settings badge; equal/older/missing target yields no false
  notice. A later higher version notifies again after dismissal.
- Manual check shows `checking` then `current`, available, offline, untrusted or
  not-published as appropriate; background checks update state without an error
  modal. Denied notification permission leaves the in-app notice intact.
- Changing the interval persists across restart and identities; fake clock,
  sleep/wake, offline, retry, 304 cache, and simultaneous foreground/background
  triggers follow the cadence rules. Android and iOS tests assert catch-up on
  next launch, not a fixed 24-hour wake-up.
- Every notice opens only a platform-pinned destination. Apple source-build
  states never claim an unsigned download can be installed. EN/RU copy and
  accessibility labels describe the same state and action.

## Checks before implementation is complete

- Publisher fixtures: server-only release, one platform only, out-of-order tags,
  draft/prerelease, missing asset, failed attestation, and rollback. Only a
  verified published client target may advance.
- Client fixtures: bad schema/host/redirect, malformed or very large version,
  older/equal/newer versions, target absent, and stale feed. Assert that no
  response-supplied URL is ever opened and no key or profile data is sent.
- Enable checks only after the Pages URL and platform destinations have live
  end-to-end smoke checks. Keep this design and the platform manuals aligned
  with the implemented behavior.
