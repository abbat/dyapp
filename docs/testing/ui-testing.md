# UI Testing: Practical Guide

Linux, Windows and macOS injected desktop smoke suites require the opt-in Cargo feature `ui-smoke`.
`linux-test.sh` and native Windows CI enable it explicitly; ordinary builds and release packages
leave it off. The feature includes script embedding, test-only IPC, isolated config paths,
reduced proof-of-work settings, video fixtures and screenshot/failure injection. The production
`locale_options` query returns pseudo-localization and a feature-only Russian override together;
ordinary builds ignore test flags. Native macOS CI enables the same desktop feature.

Release packages are checked externally: `production-desktop-smoke.py` uses Linux AT-SPI,
and the shared `production-windows-smoke` action uses Windows UI Automation. A fresh profile
must expose a visible setup heading and an enabled Create identity button from the same locale.
Linux focuses the button and quits with Ctrl+Q; Windows closes the native window. Both require
clean process exit. The artifact check rejects missing, empty or smoke-instrumented binaries.
Debian package sessions also run real WebKit counterexamples for missing/hidden headings,
disabled buttons and mixed locales; native functional flows remain in feature-enabled CI.
The clean package-test container provisions AT-SPI/WebKit introspection and xdotool.

The desktop smoke runs in one async local scope. Its explicit `interface.js` imports the real
controller modules and exposes live `appBindings` and `appApi` views; shared smoke helpers live
in a local `smoke` object. No application or test APIs are published on the page global object.
Injected imports resolve URLs against `document.baseURI`, since injected scripts have no external
script URL. The shared DOM smoke calibrates required, optional and list lookups against valid controls,
missing elements and wrong element kinds. Functional scenarios use those same controller
objects and live state bindings (see
[desktop module boundaries](../architecture/ui-state-contract.md#desktop-module-boundaries)).

Shared desktop smoke scripts, including the test-only video frame renderer and smoke checks,
live in `clients/desktop/ui-tests/` and are concatenated in its `mod.rs` order.
Native keyboard checks wait for document focus through the window focus event and a deadline;
restoring a desktop window also requests native focus. Opaque modal accessibility audits temporarily
hide background paint outside the active dialog, preserving layout and restoring inline visibility
on success or failure. This avoids axe 4.14 counting occluded page blocks as text overlap.
Normal native scenarios await one shared calibration promise at startup, before opening dialogs
or changing scroll/focus state. Calibration and audits wait for bounded paint callbacks after
fixture/style changes. Standalone prototype calibration owns its top-layer dialog. Counterexamples
still reject real low contrast and foreground overlap; failures retain layout and axe check details.
Synthetic fixtures hide unrelated page paint while preserving layout and restore it on every exit.
Ready-screen checks release prior form-validation focus before the width reflow probe,
then reveal the heading through intersection/scroll readiness callbacks. They use its live
viewport bounds and hit test, retain genuine occlusion failures, and cancel observers,
listeners and the failure-only deadline on completion.
Chat list/message, identity peer/profile and public-name/contact-action scenarios live in paired
`*-smoke.js` helpers loaded before `chats.js`, `identity.js` and `names.js`, respectively.
Those entry scripts retain their existing exported check tables. Helpers share each scenario's
fixture object; independent phases run sequentially, with each pending-response race kept together.
`pendingSmokeCall()` creates a fresh promise on each invocation and exposes its latest resolver
synchronously, so tests can settle old responses after navigation without adding polling waits.

Desktop native smokes use `openArea('settings', category)` and `openSettings(category)`
to follow the actual category buttons, including parent collections, before operating
controls. The Python desktop smoke contract checks every literal sidebar destination against
shipped buttons and section IDs; missing runtime destinations report their names explicitly.
The helper rejects a hidden destination; it does not expose panes directly
or add waits for navigation. Settings navigation has its own Back/focus/reset smoke.

Removal smoke waits for the Cancel close event before reopening, with a fresh deadline for that
transition. Cache identity-change checks also await the final close event before collection paging;
collection focus assertions follow native document focus and synchronous paging handlers, with no
extra frame wait. Identity pagination
also exercises a detached removal button: rendering uses its stable reference, preserving the
control and its handlers for a later current-identity row. Dialog readiness
requires both visible and open state, not only the hidden attribute.
The removal close handler ignores an old queued event after a newer opening becomes active.

The mixed-profile node fixture writes its LF-only network configuration as bytes on every OS;
its regression intercepts the next native launch while simulating Windows text translation.

Photo controls use labelled vector icons. Avatar tests require fallback letters to stay out of
paint while a canvas is present and reappear after removal or decode failure; own and peer
avatars share that rendering contract.

Native identifier reflow checks count overlapping vertical bands of nonempty text rectangles,
not DOM fragment rectangles: chunk spans and WBRs can produce many rectangles on one line.
The check waits for fonts, retains narrow/wide wrapping assertions and rejects horizontal clipping.

Prototype screenshots serve the mounted assets through an ephemeral loopback HTTP origin,
including the shared identifier ES module. No external network access is needed.
CI does not run it: run `make ui-prototype` locally after changing the prototype or its
helpers; it writes JSON measurements to `build/ux-research/prototype/`. D1-D3 are checked at each screenshot, including
negative fixtures for long titles, wrong Cancel keys and duplicate/missing defaults.
Selection dialogs may have no default; other dialogs must declare exactly one.
They wait for `document.fonts.ready` and two animation frames, with a
30-second failure deadline, before measuring layout. The desktop composer check changes
text through input events and compares the field's current bounds and focus as Send
appears/disappears; it does not assume a window coordinate or add a layout sleep.

Android profile-sharing coverage extends the existing two-peer request/reply test:
the peer sends a profile link, the app opens it, shares through the destination
picker, and checks delivery in the peer's history before testing card/back
navigation. On returning to the chat it waits for a rendered card with a deadline; the thread
shell alone does not mean the asynchronous history snapshot has loaded.
It also opens a link to an unpublished peer and checks the unavailable
state, retry and return to the thread. Separate dialog tests exercise empty,
failed, queued and busy states.
These checks require Android instrumentation; lint alone does not execute them.

Android `SearchOutboxJobServiceTests` exercise framework start/stop through a debug-only service
that inherits the production lifecycle and calls the real native outbox. They run on the existing
API 31 emulator: a free job ID excludes both production IDs, and the fixture owns an isolated
native registry and credential. Native admission, key-open completion, platform stop and full worker
completion each have readiness callbacks with a fresh failure deadline. The final callback follows
the completion decision, so native return alone cannot falsely prove suppression of a late
`jobFinished` request. The debug component and configurable probe are absent from release builds.
A malformed fixture database also exercises native failure and the actual retry request without
replacing its bytes. Completion flags are captured together for one execution, so legitimate OS
retries cannot invalidate assertions through growing counters. These checks do not prove elapsed
OS backoff, reboot persistence, network recovery or deadline scheduling; those runtime cases remain
planned. Host SDK compilation does not execute them.

`LocalNetworkPermissionPhaseTests` uses an isolated native identity and real Rust connection checks.
The ordinary API 31 runner explicitly executes its `legacy` phase before the UI suite. The helper
`android-lan-permission-phases.sh modern` brackets separate instrumentation processes with shell
permission grant/revoke, checking granted, denied, regranted and revoked states against an owned LAN
node. Revocation must happen outside instrumentation because the OS can terminate the app process.
Successful connections use native readiness; refusal is checked from the completed native result,
not elapsed time. Each invocation has its own failure deadline and retained report; a failed JUnit
report stops the sequence even when adb exits zero. Without a requested phase the class is skipped.
API 37 emulator provisioning, a deterministic public-address fixture and actual modern-device runs
remain planned; the API 31 phase and fake-ADB driver tests do not prove API 37 enforcement.

> **Status:** derived from `tests/scripts/ui-test.sh`, `tests/docker/compose.ui.yml`, `tests/docker/Dockerfile.android-test`,
> `tests/docker/Dockerfile.linux-test`, `tests/scripts/*-test-*`, `clients/ios/project.yml` and `clients/macos/Cargo.toml` and the test sources on
> 2026-10-06. Nothing here was run for this document; test counts are counted from source, not
> from logs. For the target contract see [empty-app-contract.md](../platforms/empty-app-contract.md);
> for the CI jobs see [ci-matrix.md](../development/ci-matrix.md).

Docker UI stages capture Gradle/emulator output in memory and print one `OK` line, or the
captured output and an `ERROR` line. `DYAPP_VERBOSE=1 make ui-test` streams live output. See
[local check output](../agents/project.md#local-check-output) for capture limits.

The Linux loopback pair also sends a public profile link through the Share dialog, reopens the
recipient, opens its real profile card through verified native fetch and follows Message back to
the writable chat. The fixture shares the sender's own public profile to avoid a third published
identity; production Share filtering and private-state/stale-fetch guards retain their separate
native and controller tests. Execution of this added node scenario is CI-only.

## Linux sticker exchange

The loopback two-client desktop scenario now includes real File/TAR import, selection in the
saved-pack picker, typed received history and emoji/name fallback after individual-media expiry.
The runner advances attachment deadlines and quarantines only the isolated sender's generated
blob files to model completed expiry, without waiting for node maintenance or changing signed
messages. The other client clicks Add pack while the sender process is stopped, verifies durable
waiting, then saves the answer after the sender relaunches and sends from that pack. Another
restart verifies the reply and hidden control-packet projection. Request/image controllers are
awaited through DOM changes and deadlines; native node passes complete before the next check.
No fixed sleep or pixel coordinates are added. These native steps execute in Linux CI; prepared
WebKit runs with mocked IPC establish controller behavior only. Release size comes from CI
artifacts, not a local native build.

## What is tested

The UI suites test the **platform shells** (`clients/android/`, `clients/ios/`, `clients/macos/`, `clients/linux/`, `clients/windows/`).
The shells call the Rust core for a peer ID (see
[ci-matrix.md](../development/ci-matrix.md#what-the-platform-jobs-do-not-validate)); shared
iOS unit tests also check per-slot Keychain wrapping account isolation and network draft
file separation/corruption. They verify that erasing one UUID wrapping item leaves another slot
and the legacy account readable, and that a valid deleting tombstone remains visible when its
slot directory is missing while malformed or symlink markers are refused. Native tests also
check confirmed removal, unknown-file and damaged-slot refusal, survivor key/draft isolation,
legacy-account preservation, failed or incomplete Keychain erase with a visible deleting
candidate, confirmation recheck on retry, and retry after partial file cleanup. iOS UI
tests cancel removal, then confirm it, select the surviving Peer ID in the picker and reopen it
after restart. A shared native unit test also reopens the surviving slot after another slot is
removed and checks its active and draft network files plus stable Peer ID. The iOS UI suite
also verifies Russian removal labels and that the double-length pseudo-language confirmation
remains hittable. The removal alert is presented from the stable Settings screen, outside its
conditional scrolling section.
Shared native unit tests additionally check that an unreadable slot cannot
replace the active selection and that a lost selected Keychain item produces recovery without
creating another key. An isolated debug UI fixture corrupts only the wrapped key file after
creation: the iOS UI suite checks the Russian unavailable label, hidden Remove action, and
recovery after restart with Retry but no implicit Create. `IdentityRecoveryTests` on iOS
seeds a legacy root identity, a malformed pointer, a missing slot or legacy key and a
pending slot in the isolated root (`--ui-test-identity-fixture=`), then checks a stable Peer ID or
recovery without Create across a relaunch. A shared native unit test also checks
that UUID profile drafts stay separate and a peer-named root draft is copied once into its slot.
The migration test also removes the slot and checks that its slot draft is cleaned while the old
root copy remains unchanged.
Another shared unit test checks that empty birth parts stay absent, yearless February 29 is valid,
and an impossible full date is rejected through the Rust UniFFI assessment.
The Apple preview model test checks that every unset field stays unpublished, partial birth parts
are preserved, and an available year produces ordered inclusive age bounds.
The preview screen is reached from the editor without an attestation; native Apple UI checks
remain in CI.
The Apple draft adapter unit test imports old JSON without deleting it, confirms `client.db` takes
precedence on later loads, and checks that another identity keeps an independent empty draft.
The Apple publication view maps the Rust signed readback to a separate, immutable snapshot before
rendering the version and public fields. The iOS UI tests save a birth month, restart,
see a localized publish failure in Russian and the double-length pseudo-language with fresh
confirmations required, then publish to the local node and find the signed readback with only the
saved fields and version 1.
No suite
exercises messaging, matching or video. The tests check launch, render, app identity and close;
the Android and iOS UI tests and the shared desktop smoke also check that the peer ID is 64
lowercase hex characters. Desktop smoke chooses each storage mode and verifies the same Peer ID
after a second launch. Before Create it checks the first-run screen: the recommended available
store is preselected, a choice alone creates nothing, no identity action is shown and no element
with the `hidden` attribute is rendered; the ready screen repeats the last two checks. File-mode cases check the visible key-file path and backup guidance;
system-mode cases check that no file path is shown. The Android UI test also recreates the activity and
checks that the peer ID, unwrapped again from the stored identity key, stays the same; the
iOS `testRelaunchKeepsPeerId` relaunches the app for the same check.
The Android place picker test picks Germany, refuses to save unlisted text, picks Berlin from
the "Berl" suggestions, checks the country and place in the publish dialog, then clears the
country and checks both are gone from the dialog. The iOS `testPlacePickerSavesOnlyASuggestedPlace` searches for
Germany, refuses unlisted text, picks Berlin from the "Berl" suggestions and previews it.
The Android publish test checks that closing the dialog clears both confirmations and an unsaved
edit disables publishing until saved, then confirms both, runs the proof without an active network
and expects the localized not-connected failure, retry label and cleared confirmations.
A second Android publish test checks and activates the emulator runner's loopback node
(`10.0.2.2:33445`), publishes a saved draft and expects the signed readback on the form, then deletes the
`network` and `network-draft` files so the other tests still start without an active network.
The Apple two-peer test (`ChatsUITests`) cannot reach the FFI from the XCUITest process, so the DEBUG
launch argument `--ui-test-chat-peer` hosts the second identity inside the app under test
(`ChatTestPeer.swift`) and shortens the mailbox poll to 5 s. The peer sends a first message once the
app's profile is listed, and confirms the app's reply with a message of its own, which the test waits
for. With `--ui-test-chat-peer-likes` the peer likes the app's identity instead and stops, so the test
can like back from Liked you. With `--ui-test-chat-peer-shares` its confirmation is the link to its own
profile. `--ui-test-metered-network` and `--ui-test-unmetered-network` replace the
real network report, because a runner's network must not decide whether the offer to load a full photo
appears. Release builds compile none of this. The peer's `NSLog` lines reach neither `tests.log` nor the
result bundle, so `apple-test-in-ci.py` saves them as `chat-peer.log`.

The Android two-peer chat tests (`TwoPeerChatTests.kt`) publish the app's identity on that node and
drive a second identity, `TestPeer`, through the FFI inside the test process: its own folder in the
app cache, its own Keystore alias, the same node and a published profile. The peer sends a text;
the app opens it from its Chats request row and replies, and the peer receives the reply. In a
second test the peer likes the app's identity; the app opens it from Liked you, likes back to
match, messages, edits and deletes the text, and the peer receives each change; the app then
blocks the peer and Chats shows the block note. After opening the match's chat, the test waits
for the composer's enabled semantics with a deadline: navigation can finish before the IO history
load, and Compose idleness alone does not cover that load.
`TestPeer.awaitMessage` checks applied history and advances through completed synchronous FFI
mailbox receives, without an artificial polling delay. Receive/history errors fail immediately.
Each message/edit/delete transition starts a fresh monotonic failure deadline; a late operation
cannot satisfy it. The deadline is checked between completed FFI stages, not a cancellation of
an in-flight native call; network requests retain their own core timeouts. `PeerTransitionTests`
uses an injected clock to cover deadline reset, late completion and error propagation without
waiting for real time. Android instrumentation CI must also run the real two-peer journeys.
The queued outcome of a send has no two-peer test:
the FFI stages only after it reached the node, so only a node refusing the write could cause it.
`ChatRulesTests` cover how a staged failure reads and `ChatScreenTests` cover its line.
The iOS and macOS network form UI tests save bootstrap rows and a network name, relaunch the app
in the same isolated slot root, and check that the values reappear in the automatic form while Ready remains
hidden. After a successful local-node Check and Continue, both clients relaunch with the same
Peer ID on Ready; the form is then available through Network connection. Shared unit tests reopen
two UUID slot folders and check that each retains its own draft and active settings.
The Android network draft tests save an invalid row, a closed port (`10.0.2.2:1`) and the local
node pinned to a foreign Peer ID under the name `city`, recreate the activity and expect the same
Peer ID and draft on the automatic form. The check marks the three rows invalid, unreachable and
wrong Peer ID, and the local node under `city` belongs to another network; Continue stays off. A
second test checks the closed port, retries with the local node, continues to the active state and
recreates the activity onto Ready with the same Peer ID. Both delete the network files afterwards.
Apple UI tests also open read-only Settings after creation, find the selected inventory row by
accessibility identifier and Peer ID, and verify the network editor link remains available.
They create a second identity, switch back to the first, and relaunch to confirm its Peer ID
persists. The later Settings matrix covers removal and error states. A DEBUG-only
launch argument seeds an invalid draft after explicit identity creation so the tests can check
the visible read error and disabled Save action; release builds ignore this argument.
They also check that a malformed bootstrap address receives an ordered row outcome after Check
and remains available on Retry. Rust's connection-check test and a desktop smoke case interrupt a silent node;
the Apple cancel path belongs to the later Apple failure matrix.
The same native UI cases verify that Continue stays disabled after an invalid row; a shared
unit test verifies that calling Activate then leaves the active settings absent.
The Linux debug UI smoke injects the synthetic raw-frame Canvas consumer, receives four binary
Tauri channel frames each at 360p and 720p, checks the final Canvas dimensions and RGBA pixel,
rejects malformed and oversized buffers, and records bounded frame counts and ages in
`desktop-ui-results.json`. Xvfb checks correctness only; it is not a performance benchmark.
The shared Linux/Windows desktop smoke blocks a known Peer ID in Settings, checks that Retry
reports the token revocation state, then unblocks it and checks that the list clears. The native
search and Liked you blocking flow remains for later UI coverage. A second identity joins the
same loopback node, and each step below relaunches its client, so state read back survived a
restart: the first identity's Like appears in the second's Liked you only and leaves with Unlike;
a pass takes a liker out of Liked you and search until Settings undoes it; a text sent after two
likes made a match reaches the other client;
the first text arrives as a request and a second one is refused until the reply opens the chat;
an edit and a delete reach the peer; Clear removes only the local history and the chat stays
writable; Block hides the chat, refuses local send and history, and the blocked peer's next text
is not stored even after Unblock. The shared Linux/Windows
desktop smoke measures the Peer ID text at narrow and
wide layout widths: it must wrap when space is tight and return to one line
when space grows, without changing the identifier. Its final check also runs the short Peer ID
rule on fixed IDs: "@" and the last 6 characters, one more for each tail another known ID shares,
and a copy of the same ID in another slot counts as no collision. On first identity creation, desktop smoke
also opens Settings to verify the active identity's full Peer ID, its short form and key backend,
creates another identity there,
switches between both identities, verifies that a changed privacy toggle stays with its own
identity, checks that Cancel receives focus in the removal dialog, cancels removal once, then
confirms removal and relaunches to
verify the surviving identity and Settings inventory. In the add-identity case, it also pauses an
old identity response, shows the newer picker, resumes the old response, and checks the picker
remains visible before returning to Ready. A late failure leaves the picker alone too, and a
current failure shows the storage error with Retry, which reopens the kept identity. It then
pauses old Search and Chats requests, starts current requests after returning to Ready, and
checks that late old results cannot change current status or busy controls. A Chats refresh after a background pass that is paused
across an identity change does not render its rows. The two-client scenario waits for a runtime
catch-up pass with Chats closed and checks that the mail reached the local history. The Chats smoke also renders local request and conversation
rows through mocked commands while the node is unavailable, marks request rows until a reply,
opens history and marks incoming text read locally, and checks Back returns to the list. Mocked
send checks the reply transition,
disabled composer for an unanswered request, retention of an unstaged draft on failure, and a
locally staged text shown as pending after node failure. The Found profile smoke opens a
`none` chat, sends one first text and checks the pending request row and disabled second send.
Late history and send results are ignored after an identity switch. The Clear chat smoke checks Cancel, a failed clear,
confirmation with an empty row and local-only note, and a late result after switching identity.
The Block smoke covers request/open controls, a failed local block, pending network revocation,
immediate row hiding and an old result after identity switch. The Edit smoke checks the action
only on live own text, current text in the editor, Cancel, refreshed text and edited mark,
retained unstaged draft, pending delivery for a staged edit and a late identity result. The
Delete smoke checks the best-effort warning, Cancel, own-message placeholder, retained text on
an unstaged error, pending delivery after staging and a stale identity result. The delivery
smoke renders only own status, refreshes node acceptance after a catch-up pass with no new text,
checks the device-received label after reopening, and checks that a pass failed after commit
still shows the committed row once. The read smoke checks that Read replaces the delivery label
on the covered text only, that a hidden window neither marks read nor sends a receipt, and that
returning to the window does both; the two-client steps check a receipt sent after turning the
setting on, its mark after a restart and silence at the default. The presence smoke checks the
mark in the row and the open chat, that a pass with no new mail removes it from both, that focus
sends presence and that a hidden window does not; the two-client steps make a match, check the
hourly throttle and wait for the mark on the other client. A later pair of steps sends 102 texts
to the stopped client, in up to four 60-second runs, and checks that all of them arrive through
the mail pass once it returns, which only works past the old limit of 100. Store tests cover
persistence
across restart
and ignore receipts for a superseded message generation. The forwarded-text smoke checks a
localized Forwarded label in the list and history, keeps the verified MLS sender as author,
and leaves ordinary text unmarked; the Linux smoke runs in English and Russian. Rust tests cover
source group/author/ID and deleted-state checks before the desktop forward command can send. The
Forward action smoke checks target filtering, Cancel, an unstaged error, queued delivery of an
edited source, no second submission after a queued result, and a
late result after switching identity. The link-host smoke checks IDN punycode, a `user@host`
spoof and absence of clickable links or previews in a message request.
The shipped page loads `chat-photos.js` before `chats-ui.js`, after Search's helpers;
`chat-message.js` then uses the controller's helper references. The smoke runs after these
classic scripts have established their shared page scope.
The chat photo smoke sends a JPEG with an added EXIF segment and checks that neither the photo nor
the preview given to `send_chat_photo` holds it, then checks the full, offline and unsupported
states and that a request offers no Attach photo.
The same case reopens a peer card and checks that old
likes and block responses cannot change the new card or its controls. It also delays the old
safety panel, camera capture and sensitive copy across a switch; an already active camera stream
stops synchronously when the switch begins. The old copy still schedules
its expiry. Paused profile load, place-name lookup and save results cannot repaint the newer
editor. An old Save or Publish preview cannot unlock the newer form or open its dialog; old
publish and hide results cannot change the newer progress, dialog or busy controls. It checks
that an old profile search-wait timer, search-send response and elapsed timer do not affect the
new editor. Late usage and status responses cannot repaint the new storage card or status bar,
or rearm old polling. It pauses privacy loads and saves across identity changes and checks that
old success or error responses cannot change the new toggles or error state. Paused blocked-peer
loads, unblock and revocation callbacks cannot change the new list, status or controls. An old
Settings identity inventory response cannot replace newer rows or show an old error. Paused node
status, settings load and save responses cannot change the new node form or unlock its Save.
A late tray result cannot replace the new window controls. Old removal inventory and profile
responses cannot open a dialog after a switch. An old withdrawal cannot remove the new identity
or change its dialog to Offline; a current withdrawal still proceeds. An old launch network
load cannot start a dial or repaint the newer connection status; a late old dial cannot repaint
it either. Old active or draft network file loads and a late draft-save error cannot change the
new network form or status. A late old check cannot update the new rows or start activation;
late activation cannot apply the node, and old error/finally cannot change the new status or
busy controls. Paused clipboard success and failure cannot alter the next identity's Peer ID,
copy label or selection; an old label reset timer is ignored. A late place suggestion for the
same text cannot replace the newer identity's suggestion ID or validity. That relaunch checks
the Storage card:
the message store path sits in the shown identity folder and has a size, no size reads as a zero,
free space shows, and the node row shows on Linux and is hidden on Windows. The status bar then
shows no traffic before any network, the message store size, and wraps without overflow at the
400 px minimum window width.
The window cases move the window, quit with Ctrl+Q, check `window.json`, relaunch to the same
bounds, then relaunch with an off-screen saved position and expect the window centred on the
primary display. The single-instance case hides the window and launches the app again: the second
launch must exit and the hidden window must come back.
On Linux the tray cases run `fake-tray.py`, a StatusNotifierItem host on the session bus that clicks
the tray menu by position: without a host the tray settings are off; the host's Quit exits the app;
close-to-tray hides the window until the host's Open, and `tray.json` keeps the setting across a
relaunch; a host that exits brings the hidden window back.
The two-client steps run `fake-notify.py`, a notification server on the session bus, on Linux: with
the window hidden and the Name and text setting on, a real mail pass must send a notice with the
escaped text, and only the server's click on it brings the window back. The summary's name is
not checked, because after a restart the sender's profile may not be in memory yet.
On Windows `windows-tray.py` opens the notification-area menu through tray-icon's hidden window
and picks Open or Quit by index; it runs the quit, close and relaunch cases, since the
notification area is always there. Before Quit it checks that the window has its small icon; the
large taskbar icon is the executable's. Before the smoke, `check-windows-icon.py` compares the 16,
32 and 256 px images of the executable's icon group with `clients/windows/icons/icon.ico`: on the
debug build in CI and on the `dyapp.exe` unpacked from the portable ZIP in the release and nightly
size workflows.
The English, Russian and pseudo-language runs check Settings actions, the active identity's
localized storage kind, and the failed network status in the selected catalogue.
It also checks that a profile with no selected fields previews an unknown age, no full-date warning,
and unchecked 18+ and public-data confirmations. It rejects publish without the public-data
acknowledgement and checks that editing clears both confirmations. A separate save/restart pair
checks a nonempty draft and its public preview. On Linux, another case uses a loopback `dyappd`
node to publish the reviewed fields with both confirmations, waits for the verified version,
then compares the node's signed profile payload with the locally saved draft and checks that the
post-publish UI readback fields, age range and full-date warning equal the reviewed preview. The
same Linux
fixture first attempts publication without an active network, checks that the draft survives,
and that search asks to connect with a retry. The disconnected Chats visit must also show a
mailbox refresh error instead of claiming the empty Liked you list is current. It then checks the node under a wrong pin, under
another network name and a closed port, each with Continue disabled and no active network; the
next launch retries, connects and retries the publication. A fresh first run connects to the same node on the
wizard's network step and must leave an active network. After it,
a search on the node shows the sample (the own profile or none) with the node's filters and no
failures; it must offer the place filter, and Germany with Berlin picked from the suggestions
finds nobody, since the published profile has no place. The smoke then publishes a second,
independent client identity with one real photo to the same node, which runs the media role, and
reads back both signed profiles; this prepares an actual peer for the interaction scenario. The
first client then finds it in search and sees the thumbnail and the full image. After activation, a node case turns on the app's own node in Settings: the form shows
its defaults, refuses a 3 MB disk limit, shows the running node ID, then stops it, and the script
checks the saved `node.json` is off. The desktop test then leaves the Network area for
Profile and comes back, checks that draft rows and failed row statuses survive navigation, and compares
the active network file byte for byte before and after a failed check. A deterministic
Rust unit test cancels and resumes a 30-bit puzzle; the UI smoke does not race the ordinary
11-bit puzzle.
Without a node, both platforms check the empty list, a closed port next to an invalid row that
activates nothing and shows the rows in the error details, a relaunch that shows Offline and
keeps the draft unactivated, navigation that keeps the draft and failed rows, and Cancel on a seed
that accepts but never answers. dyappd builds only for Linux, so on Windows the connected
(Connected again at the next launch, kept by a Cancel), wrong-pin, wrong-network and
wizard-connect and device-link paths are an accepted gap: they run the same `clients/desktop` code and are verified on Linux only. There a second client on its own D-Bus session, which keeps the single-instance check out, links to a published identity through the node, in each language and the pseudo-locale: both list two devices, a disconnect wipes the new client, and no slot stays on disk. The same
holds for publishing, searching on a node and the node settings. Without dyappd, the smoke
instead runs the no-network start of the Linux fixture on a fresh identity: publishing keeps the
draft and offers the network, and search asks to connect.

| Platform | Where it runs | Unit | UI | Result check |
|----------|---------------|------|----|--------------|
| Android | Docker (local + CI) | 6 JUnit files (`app/src/test/.../`, including birth-part and network-result checks) | 40 instrumented (`AppUITests.kt` ×5, `IdentitySettingsTests.kt` ×3, `AppearanceTests.kt` ×2, `SearchTests.kt` ×4, `ProfileDraftTests.kt` ×4, `ProfilePublishTests.kt` ×5, `NetworkDraftTests.kt` ×2, `AppLockUITests.kt` ×2, `LockSetupTests.kt` ×7, `WizardIdentityTests.kt` ×2, `PseudoLocaleTests.kt` ×3, `ActivityLifecycleTests.kt` ×1) | `check-junit-results.py`, `check-instrumentation-results.py` (≥3) |
| Linux | Docker (local + CI) | Cargo tests for shell, slots, key stores, network settings, media/stickers and bundled frontend | Shared `desktop-smoke.py` under Xvfb and GNOME Keyring; node/tray availability selects additional cases, including real sticker import/restart in full-flow mode | `run-cargo-unit-binaries.py`, `desktop-smoke.py` |
| Windows | CI only (windows-2022) | Shared desktop Cargo tests plus the credential backend and bundled frontend | Shared native smoke with platform-specific tray checks and real sticker import/restart; exact counts come from each run report | same |
| iOS | CI only (macos-15) | Swift unit files in `clients/ios/Tests/`, including Keychain and network draft cases | XCUITest in `clients/ios/UITests/` | `check-apple-results.py` (≥3) |
| macOS | CI only (macos-15) | Shared desktop Cargo tests, including login Keychain | Shared in-process desktop smoke in a Tauri `.app` | `desktop-ui-results.json` |

**Empty or skipped is failure.** Every checker rejects a missing report, zero tests, any skipped
test and any failure: `check-junit-results.py` (JUnit XML), `run-cargo-unit-binaries.py` (empty
`--list` per binary), `check-instrumentation-results.py` (needs `OK (n tests)` with n ≥ 3 and
`INSTRUMENTATION_CODE: -1`, because `am instrument` exits 0 on failures), `check-apple-results.py`
(xcresult summary: ≥3 total, 0 failed, 0 skipped, result `Passed`). A test that passes only on a
retry is [flaky](README.md#flaky-tests): `xcodebuild test` retries a failed test up to twice
(`-retry-tests-on-failure -test-iterations 3`, each try limited to 10 minutes by
`-default-test-execution-time-allowance`, so a wedged test fails instead of using up the suite's
70 minutes) and `check-flaky.py` reads the
`xcresulttool get test-results tests` tree; `am instrument` has no retries, so the Android
run reruns up to twice, each time only the tests the failed run lists (`-e class`, checked
with a minimum of one test), or the whole suite when it lists none. The tests listed by a
failed run before a passing one are flaky (`android instrumentation` when a failed run lists
none). The desktop smoke runs one app
instance per case: system create/restart, then in that profile add a file identity, restart past a
pending slot left by a simulated crash, pick from the picker after `active-profile` is
corrupted, reopen it and remove it through the remove screen; add another file identity, show
recovery after its key is corrupted and again after it is deleted (Details start collapsed,
Copy copies the code and slot row) and remove it from recovery (each removal must delete only
its slot folder and reopen the system identity); file create/restart, then
reopen the same key moved to the pre-slot root layout; a profile draft create/save/restart pair,
which rejects an unlisted place, checks that unsaved fields and its invalid state survive
section navigation, picks Berlin from the suggestions, checks it after the restart and clears it
with the country;
then Russian, pseudo-locale and an
injected rejection that hides the Profile heading. The restart in
each mode must report the same 64-character Peer ID. Full-flow desktop smoke additionally runs
`sticker-import` and a separate-process `sticker-restart` for both storage modes. A real
File/DataTransfer/change event carries a small valid TAR to the actual import command; empty
and oversized selections must not invoke it. The test intercepts only the OS chooser's default
click, so it does not automate the chooser window. Native list/page readback checks owner,
manifest and bounded RGBA before and after restart, with EN/RU import/restart captures. A
late old-input change after reopening must not import into the new manager. Locally prepared
WebKit tests exercise these browser events with mocked IPC; only native CI proves the backend
and process-restart path. Successful runs print
`UI_SMOKE_PASS:<peer-id>` (empty for the recovery case) and exit 0; failures print
`UI_SMOKE_FAIL:` with the failed UI check or error before exiting 1.
The desktop profile unit test also saves an entirely unspecified draft, reloads it, and advances
the publish puzzle after an explicit 18+ attestation; gender and every other field remain optional.
The Debian client package smoke installs and unlocks GNOME Keyring inside a
fresh D-Bus/Xvfb session and distinct test user for each binary (installed DEB
and tarball). Both binaries run the full system and file storage checks. A
separate home is needed because GNOME Keyring persists the first binary's
`primary` identity across D-Bus sessions. Without a running Secret Service the
application correctly disables that choice, making the system-mode smoke fail
rather than silently falling back to file storage.

On Linux, `linux-test.sh` keeps Xvfb server stderr in the shared check log instead of
letting `xvfb-run` discard it. When the desktop smoke fails or times out, it adds up to
4 KiB of display/session state before `xvfb-run` cleans up: the local display number,
X11 socket presence, up to eight Xvfb PIDs with their process states, and whether Xauthority
and D-Bus session settings exist. It does not dump the environment, cookie contents or
session addresses. An absent process is evidence of absence at capture time, not a root
cause diagnosis. Startup failures before the smoke starts retain Xvfb stderr and their
nonzero exit status; a display failure without `UI_SMOKE_FAIL` still fails the negative
case. There are no added retries. Successful runs keep the usual single `OK` line.
The shared runner retains raw logs and bounds displayed diagnostics as described in
[local check output](../agents/project.md#local-check-output); native CI uploads them as
`linux-check-output`, while local Docker output identifies its retained log path.

### Localized strings

Practice D1. Every visible string of the shells lives in a catalogue, and the UI tests also run
on a pseudo-locale:

| Shell | Catalogue | Pseudo-locale test |
|-------|-----------|--------------------|
| Android | `res/values/strings.xml` and `res/values-ru/strings.xml`, read with `stringResource`; plurals go in `<plurals>` | `pseudoLocalesEnabled` in the debug build; `PseudoLocaleTests` renders the start screen with an en-XA context and expects its accented strings, and renders the start screen and Settings in Russian |
| iOS | `clients/ios/DYApp/Localizable.xcstrings` (String Catalog); `Text("key")` looks the key up, plurals are catalogue variations | `testDoubleLengthPseudoLanguage` launches with `-NSDoubleLocalizedStrings YES` and expects the doubled, hittable ready text |
| Linux, Windows | `clients/desktop/ui/i18n/{en,ru}.json`; `app.js` fills each `data-i18n` element, plurals use `Intl.PluralRules` | `--pseudo-locale`: `app.js` accents every string and pads it by 40%, the smoke expects `[Réádý··]` |

The tests compare against the selected catalogue (Android, desktop) or expected language text (Apple).
`check-repository.py` fails on a Compose `Text("…")` literal under `clients/android/app/src/main`, on a
SwiftUI `Text("key")` whose key is not in the String Catalog, and on visible text in
`clients/desktop/ui/*.html` outside `<script>`/`<style>` or a `data-i18n` key missing from `en.json`.
Strings that are data, such as the peer ID, are not catalogued. English and Russian are
shipped for v0.0.1. Android and Apple use the OS app locale; desktop selects `ru.json` when
the primary browser locale is Russian and `en.json` otherwise. The repository quality check
requires every English catalogue key to have a nonempty Russian value. The Linux/Windows smoke,
Android instrumentation and Apple UI suites each exercise the Russian ready screen. The Linux
application menu entry also has a Russian description.

### Lowered proofs of work

The full profile proof of 11 bits took from 5 s to over 3 minutes on the iOS runner, about 12
nonces a second, and a slow one failed chat tests on their readback wait. So the UI suites of every
client lower it to 4 bits, a second or two, while still running the real proof. Both ends must agree:
the test node gets `DYAPPD_SEARCH_MIN_PROFILE_POW_BITS=4`, and a client gets
`set_test_profile_pow_bits(4)` from its test code (Android), its `--ui-test-slot-root` launch (iOS), or `--ui-test` with `DYAPP_TEST_CONFIG_DIR` (desktop). Clients do not read the node's hint for this
value yet, so the node setting alone changes nothing. The value also sets the minimum a search accepts, so
a test process that reads a lowered profile needs it as well. Production clients and nodes keep 11.

## Local Docker suites (Android, Linux)

Requires Docker with Compose. All commands run from the repository root.

```bash
bash tests/scripts/ui-test.sh prepare           # build images (online; also run by make prepare)
bash tests/scripts/ui-test.sh android           # = make ui-test-android
bash tests/scripts/ui-test.sh android-emulator  # = make ui-test-android-emulator
bash tests/scripts/ui-test.sh linux             # = make ui-test-linux
bash tests/scripts/ui-test.sh prototype         # = make ui-prototype; desktop prototype screenshots
bash tests/scripts/ui-test.sh prototype-android # = make ui-prototype-android; Android prototype screenshots
bash tests/scripts/ui-test.sh all               # = make ui-test; the three above in order
bash tests/scripts/ui-test.sh shell [android|linux]   # = make ui-test-shell (android)
```

`all` ends with "Local Docker UI inventory passed; Apple/Windows native CI is required
separately" — it does not cover iOS, macOS or Windows. There are no `ui-test-all` or
`ui-test-linux-gui` targets and no X11-forwarded GUI mode.

Images build only at `prepare`; `run` uses `--pull never`, and test containers run with no network
(the emulator pair uses an internal network; `android-build` is online to fetch dependencies),
read-only root, `cap_drop: ALL`, the host user's UID/GID (`DYAPP_UID`/`DYAPP_GID`), and
`check-container-isolation.sh` runs first.

### Android

- **Image** `dyapp:android-tools` (the `android-tools` stage of
  `tests/docker/Dockerfile.android-test`): Debian bookworm, JDK 17,
  Android cmdline-tools (checksum-pinned), SDK platform 37.0, build-tools 36.0.0, NDK,
  system image `android-31;default;x86_64` (Android 12, the supported floor), Rust 1.99.0 with the
  Android targets and `cargo-ndk`. It holds no source: every Android service mounts the checkout
  read-only at `/app` and `build/android-build` at `/app/build`, so a source change needs no
  image build.
- **`android-build`** (`android-test-in-container.sh build`, run first by `android`,
  `android-emulator` and `all`): copies the sources to `/tmp` with `build/` linked to
  `/app/build`, checks the wrapper jar checksum and builds the real Gradle project
  `clients/android/` (its own `gradlew`; Gradle 9.8.0 distribution checksum-verified; AGP 9.4.1
  with built-in Kotlin and the Compose compiler plugin 2.4.20): `:app:assembleDebug`,
  `:app:assembleDebugAndroidTest` and `:app:compileDebugUnitTestKotlin` with the debug core for
  x86_64 only, then a host `build/debug/dyappd`. The Gradle home (`gradle-home`), cargo home
  (`cargo-home`) and outputs stay in `build/android-build`, so a second run is incremental.
- **`android`** → service `android-unit-test` → `android-test-in-container.sh unit`: offline
  `./gradlew --rerun-tasks :app:testDebugUnitTest` on a copy of the project with the prebuilt
  core, then `check-junit-results.py`.
- **`android-emulator`** → service `android-emulator-test`, which `depends_on` service
  `android-emulator` being healthy:
  - `android-emulator` creates the AVD in `build/android-avd` (the emulator wants ~7.4G free
    for its 6G minimum userdata partition, more than the RAM `/tmp` has) and starts the emulator headless with `-accel off` by default
    (software emulation: **no KVM needed**, but slow). With `DYAPP_KVM=1`, `ui-test.sh` adds `tests/docker/compose.kvm.yml`, which passes `/dev/kvm`, adds the container user to the group that owns it on the host (`DYAPP_KVM_GID`, taken from `stat -c %g /dev/kvm`) and sets `-accel on`; locally that is `DYAPP_KVM=1 make ui-test-android-emulator`. CI also opens `/dev/kvm` with a udev rule. Health check: `sys.boot_completed = 1` and
    the package manager responds, polled every 15 s for up to 120 retries.
    Before the emulator, it generates a low-cost test key and starts that `dyappd` with the
    `store`, `search` and `media` roles (a publish uploads the draft's photos) on `127.0.0.1:33445`, advertised as `10.0.2.2:33445`, the guest's address for the container's
    loopback; its storage and logs go to `node/` in `build/android-reports`. After the suite,
    `forge-search-index.py` flips the signature of every indexed record there, and
    `SearchTests#forgedResultsAreDroppedAndListed` runs alone with `-e forged 1` (otherwise
    skipped): the search must list no profile and show the invalid-reply failure.
  - `android-emulator-test` (`ui`) installs the prebuilt debug and androidTest APKs, disables the
    on-screen keyboard (its opening moves the layout under a tap that follows typed text), runs
    `am instrument -w com.dyapp.test/androidx.test.runner.AndroidJUnitRunner`, validates the
    output, and pulls the app's `files/ui-ready.png` screenshot (must be non-empty). On failure it
    also captures `failure.png` and `window.txt`; `logcat.txt` is always captured.
  - The emulator container is started as a dependency; `docker-local.py` runs
    `docker compose stop` when the test run ends, so it is stopped but not removed.
  - Manual launcher check (e.g. the app icon): start the emulator alone with
    `DYAPP_KVM_GID=$(stat -c %g /dev/kvm) docker compose -p dyapp -f tests/docker/compose.ui.yml -f tests/docker/compose.kvm.yml --profile all up -d --wait android-emulator`.
    The debug APK is `build/android-build/android/app/outputs/apk/debug/app-debug.apk`
    on the host; the emulator container's root is read-only, so stream the
    APK in with `adb -s emulator-5554 exec-in 'cat > /data/local/tmp/app.apk' < app-debug.apk`
    (via `docker exec -i`) and `adb shell pm install`. On the 320x640 screen,
    `input keyevent KEYCODE_HOME` then `input swipe 160 480 160 60 500` opens the app drawer;
    `adb exec-out screencap -p > drawer.png` saves it. The system image has no icon-shape
    overlays, so only the default rounded-square mask can be seen.

### Linux

- **Image** `dyapp:linux-test` (`tests/docker/Dockerfile.linux-test`): Debian bookworm with GTK 3 /
  WebKit2GTK 4.1 dev libraries, Xvfb, D-Bus, GNOME Keyring and Rust 1.99.0 from a digest-pinned
  image, and the crates fetched at image build time. The image holds no build output: the
  container runs as the host user (`DYAPP_UID`/`DYAPP_GID`, set by `docker-local.py`), mounts
  the checkout read-only at `/app` and builds into `build/linux` at `/app/build`, without network. `ui-test.sh` never builds images; only a changed
  Dockerfile or `Cargo.lock` needs `make prepare` (or `bash tests/scripts/ui-test.sh prepare`).
- **`linux`** → `linux-test.sh`, after the container isolation check. With `--features ui-smoke`, it builds the app,
  the test-binary inventory `build/unit-build.json` and `dyappd` (a no-op when current), which
  is also how CI's **native - linux** runs it.
  Under `xvfb-run` + `dbus-run-session`, it unlocks a test GNOME Keyring, runs the
  unit binaries with `run-cargo-unit-binaries.py`, then `desktop-smoke.py` launches
  `build/debug/dyapp-linux` 45 times using isolated test profiles. The network cases check an
  empty list, a refused and a malformed seed row, and that the draft survives a restart unactivated.
  The loopback cases start `build/debug/dyappd` (built by the image, or by the script when
  missing), connect the form, activate the network, publish the profile and find it as the only search result. Then the script flips the
  signature byte of that record in the node's search index: the next search shows no person
  and lists the forged reply. Hide from search then leaves a tombstone on the node and the
  hidden flag in the store. With the node stopped, removing the identity offers Retry and Remove
  anyway and Cancel keeps it; with the node restarted the tombstone goes first and the identity is
  removed. Settings → Privacy starts at its defaults and keeps changes across a restart. This
  isolated fixture uses an 8-bit node-ID puzzle in both the node and test-mode client; production
  keeps 22 bits. The profile test saves a valid draft and checks its fields after a normal app
  restart; cancellation of the publish dialog keeps that saved draft. Unsaved crash recovery is
  outside this test and is described in the [UI state contract](../architecture/ui-state-contract.md).
  The contact safety check opens a verified search result, asserts the grouped 60-digit number
  and checks that the rendered QR canvas has a white quiet zone and black finder pattern. The
  QR payload comes from the active identity and selected contact. A Rust unit fixture renders
  that payload into grayscale pixels and checks the bounded decoder's match, reversed/foreign,
  bad-version, blank-frame and multiple-code results. The native UI test also feeds a PNG made
  from its displayed QR through image import and rejects that same-direction code; it reports a
  malformed PNG. Fake media-device responses verify camera denial, no-device
  status and cancellation of a pending capture without opening a real camera.
  Real Debian 12 capture and two-client scanning remain pending.
  Opening the panel leaves `view-verified` hidden, `verify-dialog` opens with Cancel focused and
  Cancel changes nothing, and the real `confirm_contact` refuses the own-profile fixture. The stale
  identity guards answer the command from the page: each error code keeps the mark off, an answer
  for an earlier identity changes nothing, a current answer shows the mark and the fresh profile,
  and a failed removal keeps it. Rust unit tests cover the write and the read of the mark; a real
  two-client confirmation remains pending.
  A fake-clock clipboard case copies the grouped number, runs the 60-second
  expiry through the native clipboard command, preserves a later different
  safety number, and leaves an ordinary Peer ID copy alone. It tests the
  plugin in CI's desktop session; clipboard-history exclusion still needs a
  real desktop-session check.
- **Screenshots.** Smoke scripts live in `clients/desktop/ui-tests/`, outside the shipped
  `ui/` assets; `ui-tests/mod.rs` defines their evaluation order. The debug shell concatenates
  the registered helpers into one WebView script, with `main.js` last to dispatch named cases.
  `removal.js` covers offline withdrawal and tombstone removal; a failure reports the returned
  state and remaining identity count. The earlier files expose helpers in the same eval scope.
  `ui-test.sh linux` empties `build/ux-research/current/` (gitignored), mounts it at
  `/shots` and sets `DYAPP_UI_SHOTS`. At named checkpoints the injected smoke asks the test-only
  `ui_test_shot` command which widths are missing, resizes the window to 440 and 1280 px wide
  (1400 px high) through `ui_test_window`, which waits for the native bounds, waits two animation
  frames, has `ui_test_shot` run ImageMagick `import -window root` and restores the window; the
  Xvfb screen is 1280x1400. Files are
  `<state>-<lang>-<width>.png`; the first capture of a state wins. After the English pass,
  `linux-test.sh` runs `desktop-smoke.py` again with `DYAPP_SMOKE_RU=1` (every case gets
  `--ui-test-ru`) for the `-ru` files, so the screenshots roughly double the desktop smoke time.
  Without `DYAPP_UI_SHOTS` the command does nothing. A failed or empty screenshot fails the UI case.
  States: `setup`, `wizard-network`, `picker`, `ready`, `settings`, `storage`,
  `settings-storage`, `remove`, `remove-offline`, `recovery`, `recovery-details`,
  `network-empty`, `network-failure`, `network-connected`, `profile-edit`,
  `publish-dialog`, `profile-publish-failed`, `profile-published`, `profile-hidden`,
  `profile-unhidden-pending`, `search-not-connected`, `chats-not-connected`, `search-results`,
  `search-profile`, `search-safety-confirm`, `search-safety`, `search-safety-verified`,
  `search-empty`, `search-error`, `chat-new-device`, `statusbar`, `tray-none`, `tray` (where
  the test flow reaches them). The in-progress network check and the publish puzzle finish
  too quickly to capture.
- **Accessibility audit.** Linux and Windows CI supply checksum-pinned axe-core 4.14.0
  through `DYAPP_AXE`; a missing, unreadable or wrong-version engine fails. Before every
  named smoke screenshot checkpoint, including runs without `DYAPP_UI_SHOTS`, the shared
  `criteria-axe.js` audit enables every WCAG 2.0–2.2 A/AA tagged rule explicitly, including
  disabled-by-default `target-size`. It runs in light and dark themes and reports every
  affected element. Contrast/target-size results requiring review also fail. At the first
  checkpoint it verifies that low-contrast text and adjacent undersized buttons are rejected.
  The existing native checks also reject positive `tabindex`, missing focus rings and
  horizontal overflow at 440 px. Mobile native accessibility audits remain deferred.

### Desktop prototype screenshots

`ui-test.sh prototype` (`make ui-prototype`) uses the lightweight `dyapp:prototype` image
(Debian 12, WebKitGTK 4.1, Xvfb and pinned axe-core; no Rust build). Assets and Python helpers
are mounted, so code edits do not need an image rebuild. Every desktop state is captured in
English/Russian, light/dark, at 320/440/1280 px and normal/200% root text size. An additional
`en-XA` pass at 440 px uses the Spectrum text-expansion bands, with the frozen 70+ character
boundary at 130% of the original length. Captures are 800 px high and save the full page.

Normal light filenames remain `<screen>-<lang>-<width>.png`; variant suffixes are `--dark`
and `--text200` before the language. Double hyphens prevent a dark variant of `profile`
from overwriting the distinct `profile-dark` state. Duplicate names fail before any work.
The script accepts `desktop` followed by optional state names. Use a fresh `/shots` mount
under `build/` to preserve prior captures. `DYAPP_PROTOTYPE_THEME` (`light`/`dark`),
`DYAPP_PROTOTYPE_TEXT_SCALE` (`100`/`200`) and `DYAPP_PROTOTYPE_LANGUAGE` (`en`/`ru`)
select a matrix slice; each defaults to `all`. The English slice includes `en-XA`.
Loading, font/layout readiness, criteria, snapshots and keyboard checks advance only
from their completion callbacks. Each phase gets a fresh 120-second failure deadline;
it never advances a test. Completion cancels the prior deadline. The raw log names the
current shot and phase, and a terminated WebKit process fails immediately.
Startup markers distinguish the launcher, Xvfb, GTK and WebKit initialization. A native
Python watchdog also covers blocked initialization or a blocked GTK event loop; only
verified phase progress resets it. It emits thread stacks and exits on failure, and is
cancelled after report completion. Ordinary successful output stays captured.

Each capture writes axe, clipping and dialog findings to `criteria.json`; missing measurements
fail. Known accessibility/reflow findings live in `tests/scripts/desktop-ui-baseline.json`.
New findings and stale entries fail. The baseline is intersected with every committed version,
so it only shrinks, including manual CI runs; full Git history is required. Dialog and keyboard
failures cannot be budgeted. `check-ui-criteria.py` validates each slice against its expected
shots and the complete matrix. The existing L2/I1 measurement stays in the frozen normal-text
English/Russian 440/1280 light matrix and is checked through `layout.json` and `[layout]`.

The frozen matrix also walks real GDK Tab/Shift+Tab events in DOM order and uses arrow keys
for every native radio-group member. Rendered focused/unfocused pixels must meet WCAG 2.4.13:
3:1 contrast over at least a two-CSS-pixel perimeter area. Entirely obscured focus fails
SC 2.4.11. Escape closes dialogs and restores the declared opener; unsolicited revocation
focuses the next screen heading. Counterexamples prove failures for traps, invisible focus
and ignored Escape, in addition to axe, dialog and clipping fixtures. A focus-area failure
also saves the last compared frames as `keyboard-focus-before.png` and
`keyboard-focus-after.png` beside the captures. The prototype models
these keyboard/dialog transitions without backend actions. D1's two-line limit stays in the
frozen matrix; stress variants check clipping and accessibility instead.

The prototype check runs only locally (`make ui-prototype`); CI does not run it. Set
`DYAPP_PROTOTYPE_THEME`, `DYAPP_PROTOTYPE_TEXT_SCALE` and `DYAPP_PROTOTYPE_LANGUAGE` to check
one theme, text scale or language.

### Android prototype screenshots

`ui-test.sh prototype-android` (`make ui-prototype-android`) does the same for
`clients/android/prototype/index.html` with the same image: every state of
[Mobile screens](../design/mobile-screens.md) in English and Russian, light and dark, at 360 and
412 px wide, saved to `build/ux-research/prototype-android/<state>[-dark]-<lang>-<width>.png`.
The desktop prototype is mounted too, because the Android one loads its strings. The layout check
runs with the `android` baseline.

### Reports on the host

Reports are written to `/reports/<container hostname>/`, a bind mount of `build/linux-reports`
(Linux) or `build/android-reports` (Android) that `docker-local.py` creates as the host user. `ui-test.sh` does not pass `--rm`, so the last containers remain and can be
exported:

```bash
python3 tests/scripts/export-ui-results.py android <dir>   # android-unit-test + android-emulator-test
python3 tests/scripts/export-ui-results.py linux   <dir>   # linux-test
```

The exporter prints `OK ui-export` or mapped failure details. `--raw` keeps
Docker diagnostics; exported reports and application files keep their formats.
`--help` describes the command without accessing Docker.

| Suite | Exported files |
|-------|----------------|
| `android-unit-test` | `gradle-version.txt`, `test-results/` (JUnit XML), `hs_err*.log` on JVM crash |
| `android-emulator-test` | `instrumentation.txt`/`.json`, `instrumentation-<n>.txt` of failed runs, `ready.png`, `logcat.txt`, on failure `failure.png`, `window.txt`; plus `app-debug.apk` |
| `linux-test` | `desktop-ui-results.json`; plus the `dyapp-linux` binary |

In CI the Docker jobs upload **no** artifacts (logs only).

## Native CI suites (iOS, macOS, Windows)

These run only in `ci.yml`; see [ci-matrix.md](../development/ci-matrix.md#ciyml-jobs).

- **macOS:** Cargo runs the shared desktop unit tests with `DYAPP_TEST_KEYCHAIN=1`.
  `build-macos-client.py --profile debug --features ui-smoke` assembles a real
  `.app`; `desktop-smoke.py` tests its bundled executable. Native window events
  signal resize/hide/show readiness. The Mac driver uses process-bound markers
  and kqueue for Dock reopen and registered tray callback coverage, without
  coordinate clicks. Release builds omit these test hooks.
- **iOS:** XcodeGen 2.46.0 generates `clients/ios/DYApp.xcodeproj` after
  `build-apple-core.sh ios`. `apple-test-in-ci.py` builds the device app and runs
  XCTest/XCUITest on an available iPhone simulator, signed ad hoc for Keychain.
  `check-apple-results.py` and `check-flaky.py` validate its nonempty inventory.
  The artifact also keeps the runner's `DYApp*` crash reports in `crash-reports/`: a test
  that ends with "application is not running" says nothing about why, and the report does.
  When there is a crash report, `app-errors.log` keeps the app's error, fault and exception
  log lines: a report for an uncaught exception may carry neither its reason nor its backtrace.
  For a macOS window that does not open, `gh run rerun <id> --failed --debug` adds
  `window-diagnostics/` to the artifact: the saved state folder, the app's defaults, and the
  app's system log lines about windows and restoration. Ordinary runs skip this step.
  The DEBUG-only `--no-device-passcode` launch argument makes the iOS missing-passcode
  lock test deterministic across simulators with different credential settings.
  The DEBUG-only `--ui-test-corrupt-network-draft` argument seeds a malformed draft in the
  selected UI-test slot after creation; it never runs in release builds.
  The DEBUG-only `--ui-test-fail-publish` argument makes profile publication fail as if the
  active network were unavailable, while the local node keeps running for later cases.
  Before XCTest, the runner builds a host `dyappd`, generates a low-cost test key and starts
  it on `127.0.0.1:33445` for the native network UI cases. Its node ID, log and build output
  stay in `$RUNNER_TEMP/apple-results/`; the runner waits at most 30 seconds for its loopback
  listener and terminates the process after tests. It never contacts a public bootstrap node.
  Shared native unit tests first discover the local node's Peer ID, then check a pinned address:
  the active file must be absent before Activate and contain that address afterward. iOS
  UI tests show Continue disabled before Check, enabled after the local node answers, and
  open the profile editor after explicit activation.
  The failure cases use `127.0.0.1:33446` as a closed loopback endpoint, alongside malformed,
  repeated and invalid-pin rows. They assert the row order and keep Continue disabled when no
  compatible node answered.
  A second local listener on `127.0.0.1:33447` accepts TCP without answering the peer handshake.
  Shared native tests cancel that pending check and retry it while asserting the previously
  activated network remains unchanged. iOS UI cases cancel, retry, and reopen the same
  saved draft after a process restart.
  Another shared unit case derives a valid wrong pin from the local node's Peer ID and checks
  both wrong-peer and wrong-network results with fresh sessions; both must reject Activate.
  iOS UI cases show the wrong-network row and disabled Continue for a named network
  against the public loopback node.
  `apple-test-in-ci.sh` exits 2 unless `GITHUB_ACTIONS=true`, so there is no supported local
  command. On a Mac you can run `tools/scripts/build-apple-core.sh`, generate the project with
  XcodeGen and run the scheme in Xcode yourself; that path is not scripted.
- **Windows:** `cargo build` + `cargo test --manifest-path clients/windows/Cargo.toml`, then
  `desktop-smoke.py` on the exe; artifact `windows-app-and-ui`. Locally on Windows:
  `cargo test --manifest-path clients/windows/Cargo.toml --locked` and
  `python tests/scripts/desktop-smoke.py build/debug/dyapp-windows.exe` (report goes to
  `%RUNNER_TEMP%` or `/reports/%HOSTNAME%`, so set `RUNNER_TEMP`).

### Writing Apple clients without a Mac

Swift compiles and lints only in CI, so these save a round trip of about 30 minutes:

- A new Swift file goes into both `project.yml` files. The macOS unit test target lists its files;
  the other test targets take whole folders.
- A push cancels the running `ci`, so hold it while a run is the evidence you are waiting for.
  The job log prints only failures and the first line of a multi-line `app.debugDescription`; the
  element tree is in `tests.log` of the `apple-<platform>` artifact. That artifact holds thousands
  of files, so read the one log in seconds with `python3 -I -B tools/scripts/artifact-member.py
  <run> apple-ios build/ai/DIR tests.log flaky.log`, which sends HTTP range requests through
  `gh api`; `DIR/names.txt` then lists every member.
- A macOS `Form` does not scroll on its own, and a tall one runs off the top of the window. Use
  `ScrollingForm`. CI crashes the macOS app now and then on "more Layout Window passes than there
  are views in the window": a cursor hit test in the display cycle updates SwiftUI's
  `HostingScrollView` under the pointer, which resizes again, right after the setup's Continue.
  A fixed `TextEditor` height did not stop it; the cause is still open. A grouped `Form` on macOS
  13+ is no way out: there a click on the network form's Check connection did nothing, and every
  UI test through setup failed. With the pointer parked on the status bar after Continue the crash
  stayed away, so the network form, under the pointer at that moment, is a plain `Form` now
  (no crash in the three CI runs after it).
- All links of the start screen share one `selection`. On a macOS sidebar a link held active by its
  own flag keeps the detail pane, so no other link can replace it.
- On macOS 12 a `NavigationLink` inside the detail column replaces it with no back button, so
  sub-pages there are state with an explicit Back, and a link to a page the sidebar already has
  (the network form from settings or search, the profile after Continue) selects that sidebar
  entry. A list picked from inside a form (the country list) is a sheet with its own field and
  Cancel: a sheet has no toolbar, which is where `.searchable` puts its field on macOS. On iOS,
  `.task` on a root row is cancelled when a page is pushed, so polling lives on the root
  `NavigationView`.
- `.task` on a `Section` is unreliable; put it on the first row. Buttons inside an iOS form row
  need `.buttonStyle(.borderless)`, or one tap fires all of them.
- `TextField` inside `.alert` needs iOS 16 and macOS 13, so rename screens are sheets.
- Keys: `Text("key")` literals must be in `Localizable.xcstrings`, and interpolation looks up a
  `%@` key, so concatenate instead. `Button(cond ? "a" : "b")` needs `LocalizedStringKey(...)`.
- `.alert` and `confirmationDialog` on macOS are window sheets: `app.sheets.buttons["Label"]`,
  against `app.buttons["Label"].firstMatch` on iOS. A switch value is the String `"0"` or `"1"` on
  iOS and an `Int` on `app.checkBoxes[id]` on macOS. `.textSelection(.enabled)` text may appear in
  `value` instead of `label`.
- An `accessibilityIdentifier` on a form can overwrite its children's; add
  `.accessibilityElement(children: .contain)`.
- Use the `reveal` and `flip` helpers in `UITests/Reveal.swift` for off-screen elements.
  `isHittable` is true for a row under the iOS navigation bar or past the form's end above the
  App lock button and the keyboard, so `reveal` compares the row's frame with that visible area
  and prints a `reveal:` line in `tests.log` for each row it has to move. A form row below the
  screen or the keyboard does not exist, so reveal it before waiting on its state; a wait in place
  times out, and its polls log `Capturing element debug description`. A row of a screen still
  sliding in has an empty frame, on which `isHittable` fails the test, and so do `frame` and
  `isHittable` on a row that vanished since `exists`. So `reveal` reads the row through
  `snapshot()`, which throws instead, and calls `isHittable` only for a row it sees on screen.
  When the drag that moves a row loses it, `reveal` first looks past the edge it moved the row
  toward.

## Not covered

- UI coverage (JaCoCo, Xcode coverage) is not collected; see
  [Coverage policy](README.md#ui--platform-coverage).
- The Android emulator runs with `-accel off`, without KVM, unless `DYAPP_KVM=1` is set; CI
  sets it.
- Product-level UI flows (onboarding, profiles, matching, messaging) have no tests yet.
- Desktop window and tray behaviour
  ([state table](../design/v0.0.1-client-screens.md#desktop-window-and-tray)) needs a real
  session: in Docker the tray host is `fake-tray.py`, which clicks the menu over D-Bus, and on
  Windows `windows-tray.py` posts the icon's messages instead of clicking the taskbar, so real
  panels, display changes and the macOS Dock are checked by hand on each OS.

## Troubleshooting

Linux unit/smoke stages, the standalone desktop smoke CLI, the Ubuntu release-tarball
smoke and Windows Cargo checks use mapped concise output. Native Linux/Windows CI archives full
logs from `build/platform-check-output`; JSON unit manifests and desktop result
files keep their existing roles. If the Linux test binary fails to compile, the script prints up
to eight compiler errors from its JSON manifest into the mapped failure output. Linux CI also
archives native Clippy JSON, stderr and its complexity report. Desktop smoke failure markers
preserve up to 8192 characters on one line, including the underlying axe checks. Desktop failures
include only the failed app invocation output, so the expected `--break-ui` rejection is not
mistaken for a regression. Prepared Rust unit binaries retain their actual failure exit code
(including 101) and reject empty inventories. Add `--raw` to `linux-test.sh`,
`desktop-smoke.py` or `run-cargo-unit-binaries.py` for direct diagnostic mode. A desktop page
check that does not finish in 100 s fails with its own name, so a stub that never answers is
not reported only as a process timeout; the two `add` cases allow 300 s for the whole step.

Apple hosted tests and core builds also use mapped results, with raw logs in the
native artifacts. `apple-test-in-ci.sh` keeps its exit-2 rejection outside approved
CI; the Python frontend likewise refuses local execution. `--raw` selects direct
diagnostic output for these Apple entry points. Existing SDK-free command-plan
tests use raw mode; actual native builds/tests run only in the approved CI.
Android unit/UI work remains enclosed by Docker capture; the emulator itself is
a streaming service, not a finite quiet check. CI captures Compose service logs
and copies the Android reports through the retained unit-test container
using `export-ui-reports.py` (`--directory PATH` selects a different destination).
Unique directories under `build/platform-reports`
retain XML/JSON, screenshots and validator logs without deleting earlier exports.
Missing containers and empty copies fail. The `android-check-output` artifact
contains those reports and `build/platform-check-output` raw logs.

- **Emulator never healthy:** software emulation boot can take many minutes; the health check
  allows ~30 min. Look at the `android-emulator` container logs.
- **`OK (n tests)` with n < 3 rejected:** a test class was dropped or filtered; the checker requires
  the full instrumented inventory.
- **`Empty unit suite` (Linux):** a test binary in `build/unit-build.json` lists no tests; rebuild the
  image after changing tests (`ui-test.sh prepare`), since the inventory is created at image build.
- **Stale results after code changes:** the Linux and Android suites build from the mounted
  checkout; other images copy their sources at build time, so rerun `prepare` for them.
