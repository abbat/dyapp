# UI Testing: Practical Guide

Localized native hosts wrap their context to supply actual configuration to child windows.
Theme and appearance code resolve the Activity through AndroidX `LocalActivity`, never by
casting a View context. `LocalizedDialogScaleTests` exercises the real wrapped host and
checks both Activity ownership and the native View font scale before accepting themed content.
The app-lock restoration fixture creates one controlled IO gate per startup owner, waits for
actual read entry before releasing it, then asserts the exact initialization history and retained
private saveable state. Every owner/read/completion transition receives a fresh failure deadline.
Fresh-identity chat and receive fixtures use an owned `NativeSlotFixture` storage context;
`skipNetworkEntry` alone reuses existing device identities and cannot establish empty storage.
The chat empty-state test checks its native registry and chat rows, retains the no-badge assertion,
and verifies the displayed peer against that same owned identity. Fixture context wrappers retain
the caller's Activity chain while overriding only application storage.

Real-IME owner checks refetch control semantics on every readiness attempt and combine those
measurements with the current native window transform and visible frame. A failed containment
check reports the control tag and measured rectangles; stale pre-keyboard geometry cannot establish
readiness or obscure a persistent enlarged-text layout defect.

`ChatLinkHostsTests` renders actual message bubbles through the shared JNI URL projection. EN/RU
captions retain ASCII/Punycode, nondefault ports and first-occurrence deduplication, exclude misleading
credentials, update after edits, and disappear for deleted/null/no-web-link text. Normal and 200%
font cases check actual text layout; completion uses Compose synchronization, without sleeps,
network requests or a Kotlin parser fixture. Full native visual review remains separate acceptance.
`ChatLinksVisualTests` supplies the dedicated `linksVisual=1` capture fixture for production chat
layout in live/edited/deleted states, with actual native text-scale and geometry witnesses. It is
excluded from ordinary instrumentation; capture-runner and artifact-validator adoption is planned.

`NativeClientUpdatesTests` checks the actual JNI metadata bridge: the native installed version
matches `BuildConfig.VERSION_NAME`, fresh state never claims current, and all four installation
intervals survive a separate native handle. This fixture performs no HTTP and does not enable
update controls; live-feed, lifecycle and notice acceptance remain separate.
`UpdateMetadataCardTests` gates read completion explicitly, rejects a fabricated selected
interval during loading/failure, and checks that an out-of-range Unix timestamp cannot wrap
into a misleading date. `UpdatesCardTests` waits for the actual persisted selection before
asserting the disabled pre-publication controls. Owner replacement/disposal is covered by
the shared `NativeReadTests` used by this card. `UpdateAvailabilityTests` renders
typed core current/package/source states and absent/unsupported/malformed cases,
asserting localized copy without any version comparison or network request.

Private profile-view checks separate presentation lifetime from core policy. Android
`ProfileViewDisclosureTests` waits on controlled IO entry/completion signals and checks readiness,
disposal, replacement and exactly one invocation per opening. Native Rust checks cover actual
owner/context/default-off/block admission; they do not become accepted merely because Compose
fixtures pass. A channel-gated native regression holds the real shared contact mutex while both
opening factories must complete before its release, then rejects both original tokens after
context invalidation. Core observer controls retain unique-generation/ABA and fail-closed poison
checks; no elapsed delay establishes capture readiness. `node tools/web-lint/profile-view-fixtures.mjs` exercises the exact dependency-free
browser controller with held Promise completions and per-case failure deadlines. The native
desktop smoke additionally drives the actual profile page and verifies close/area retirement;
none of these controls substitutes for encrypted two-peer delivery and received-list acceptance.
The Android received-list hint uses `profile_views_received_hint`; the privacy policy uses
`profile_views_hint`. Keep these resource names distinct across all `values*` XML files. The repository checker rejects
duplicate main string declarations, including aliases, within each qualifier across feature XML files;
translations and other qualifiers remain independent.
The Docker APK build retains Gradle stack traces on failure so dependency-resolution errors
can be investigated from its captured build log.
`ReceivedViewsCollectionTests` renders the production Android collection and native pager with
controlled read completions: loading, exact-target search, failed-refresh retention, explicit retry
and retired-owner actions. Loading copy names the received-view list; the profile-loading resource
belongs to profile fetches. Captured row callbacks must also refuse after query replacement removes
the row, while the current target still opens. The shared collection-row geometry matrix includes its full identifier
and UTC date/Open action in EN/RU, light/dark themes, at normal and enlarged text. Collection rows
reuse `IdentifierText` with the receiving/device UI: fixed display groups, exact raw accessibility
text and the actual grouped text-layout callback. Geometry checks reject line breaks inside groups;
visible Peer ID/local-entry labels and spacing distinguish identifiers from status/action prose. The identifier
layout must expose the actual requested native font scale, and the full date must fit below it. These are runtime checks, not evidence of
fresh rendered acceptance until the corresponding source-bound Android CI has executed them.
`node tools/web-lint/received-view-fixtures.mjs` checks the dependency-free received-list request
controller with explicitly released promises: core page/cursor pass-through, pending-action refusal,
refresh failure retention, query replacement and retired completions. The CI web entrypoint invokes both Node fixture suites through the mapped quiet runner with the
explicit spec reporter; real assertion-failure controls verify test names, source and errors survive.
The controller owns UI request lifetime only; the shared Rust pager retains filtering, counts and cursor policy.
The native smoke bridge regression checks every direct `appBindings`/`appApi` reference against
its declared getter/setter namespace before building a webview; runtime checks still prove behavior.
The Tauri received-view smoke drives the actual Privacy pane: read-only loading, unchanged native
cursors/dates, query reset, failure retention, explicit profile opening and Back focus after read
completion. A held refresh completes after leaving the pane and must not replace known rows or
admit a captured old row. These native webview checks run in desktop CI. Hidden-ancestor controls hide only the identity
shell, without an area/identity event: a late disclosure preparation cannot submit, and a connected
old received-row button cannot fetch a profile. Guards observe actual rendered rectangles rather
than child-only hidden flags; each control restores the original shell in `finally`.
The five Android update-dialog prototype outcomes run `update-fixture-tests.js` after capture.
Five `update-notice-permission-*` Settings outcomes run `update-permission-fixture-tests.js`,
covering explicit readback completion, refusal, read retry, pending duplicates and retired callbacks.
These presentation samples never request platform permission or activate feed checking.
The seven Android `search-recovery-*` prototype states run `search-recovery-fixture-tests.js`:
explicit completion releases pending actions, stale reviews require a fresh read, delivery retry
cannot confirm again, and retired callbacks cannot revive a card or change editor fields.
These samples prove presentation behavior only; core/native owner authority remains separate.
Controlled completion signals check pending admission, failed acknowledgements, browser failure/retry,
exact external destination, persistent availability badge and retired callbacks; live native/core
acceptance remains separate. Actions use the existing current-geometry reachability helper.
The five Android `settings-views` prototype outcomes run `views-fixture-tests.js` after capture.
Loading copy describes the local list read. An empty collection has one dedicated status; a
search miss in a populated collection retains distinct no-match feedback. Their checks cover these
states and the actual CSS-token gap between search/status and date/Open, alongside read-outcome
rows/errors, full-target search across bounded pages, explicit Open and
current-geometry control reachability. Non-shipping samples reuse existing prototype collection
navigation; production Android and Tauri filtering/paging remain owned by the Rust core.
`ReceivedViewsVisualTests` is a dedicated `viewsVisual=1` capture phase for all five read outcomes,
in EN/RU, both themes and actual 100/200% text. It uses the production Settings frame, collection
and native pager with controlled readers, waits for outcome/pager readiness, and verifies every
laid-out text line through current scroll geometry. Top/bottom PNG frames retain the exact producer
SHA, actual native density and viewport dimensions; these controlled samples prove rendering only.
`NativeScreenVisualOutput` writes this metadata and the existing receiving matrix's unchanged format,
rejecting reused directories, duplicate frames and incomplete frame counts.
Use the manual `android-visual` workflow's `suite=views` with an exact successful main APK producer.
Each 360/412dp job runs this phase alone; existing `standard` and `comparison` captures remain
separate. The bounded `android_visual_archive.py --kind views` validator requires all eighty
frames, exact producer SHA, locale/theme/font/outcome matrix, actual text-scale witnesses, matching
PNG dimensions and the dedicated archive namespace. A matrix proves captured controlled rendering,
not live private delivery, and still requires review of the frames.
`NativeProfileViewDeliveryTests` separately exercises actual generated JNI and first-contact
mail delivery between existing test peers: disabled/retired opening refusal, explicit View,
recipient readback with its own sending disabled, one-shot replay and no Like/chat/notice side effect.
Receive completions advance its readiness checks. UTC-day deduplication stays in fixed-clock core
tests; this runtime test does not assume the wall clock remains on one day. Execution belongs to
Android CI, separately from controlled rendering acceptance.
The independent `suite=updates` phase uses `UpdateNoticeVisualTests` with `updatesVisual=1`.
It renders the production update dialog with controlled core records for package/source offers,
pending dismissal, write failure and an actual controlled browser exception. Five additional
`update-notice-permission-*` outcomes render the production update Settings card inside the About
frame: allowed, blocked, pending launcher, failed launcher and failed permission query. Launcher
outcomes use the real ActivityResult registration with a controlled registry; no system settings
are opened. Each width requires 160 EN/RU/theme/font/top-bottom frames under
`files/update-notice-visual/<width>/`;
`android_visual_archive.py --kind updates` enforces the exact source, inventory, real native
text-scale witnesses and PNG/viewport dimensions. Its regression independently enumerates all ten
native dialog/permission states and rejects incomplete manifests at both widths. Geometry checks expose every text line through
current semantic scroll viewports and require both actions to remain visible. Run it with an APK
producer containing these fixtures; authored checks are not captured/reviewed evidence and do not
prove native persistence, live publication, OS notifications or update activation.
`UpdatePermissionResultTests` supplies a controlled ActivityResult registry to the production
permission row. It retains real launcher registration/disposal while releasing results explicitly:
a cancelled result must trigger an actual permission read, repeated pending actions cannot launch
again, and an old opening cannot consume results or launch into its replacement. Controlled
LifecycleRegistry transitions also verify foreground permission readback, visible query failures,
detached disposed observers and pending-launch admission preserved across pause/resume. These checks
open no system settings and do not substitute for actual Android permission enforcement.
Permission-row and search-recovery retirement tests capture the guarded application action through
`applicationClickKey`; a retained Foundation `OnClick` wrapper may access a detached Modifier node
before reaching application admission. A retired permission row also refuses failed-read refreshes.
`UpdateNotificationTests` requires immutable Activity pending intents on every supported device
(minimum API 31), with no version branch that could skip those assertions. It also checks that
opening the exact core-provided release URI adds no installer, identity or private payload.

The independent `suite=activity` runs `PresenceVisualTests` with `activityVisual=1`.
It captures production list/header activity marks and the audience picker in the actual Privacy
Settings frame: active/unknown list and chat, matches/nobody/loading/read failure. Each width requires
128 EN/RU/theme/font/top-bottom frames under `files/activity-visual/<width>/`; the
`android_visual_archive.py --kind activity` validator requires exact source and actual native
text-scale witnesses. Unknown projections omit marks, loading forbids edits, and failed reads offer
read-only retry. The host has no identity/network owner or peer-photo loader, so these samples cannot
announce activity or fetch profiles. Capture/frame review complements the separate actual JNI
presence-delivery and lifecycle tests; authored fixtures alone do not prove rendered acceptance.

The independent `suite=recovery` runs `SearchRecoveryVisualTests` with `recoveryVisual=1`.
It captures the production search-hiding recovery card for review, busy confirmation, queued,
acknowledged, read failure, confirmation refusal, retry failure and the five typed review refusals.
Each width requires 192
EN/RU/theme/font/top-bottom frames under `files/search-recovery-visual/<width>/`, validated by
`android_visual_archive.py --kind recovery` with the exact producer and real text-scale witnesses.
Recovery uses the same completed remote tar and SHA-256-verified transfer as every native visual
phase. The collector regression exercises every archive declared by the capture runner; a passing
instrumentation test cannot substitute for its required archive.
Controlled callbacks retain busy states until capture finishes; cleanup awaits admitted completion.
Fixtures have no registry or credentials and cannot sign or contact a node. Actual JNI authorization
and manual review of these captures remain separate acceptance requirements.
`SearchRecoveryFailureTests` covers refusal, fresh-review admission and retry without re-signing.
`SearchRecoveryUITests` keeps the real editor/name field beside the card and checks that queued
completion and closing preserve unsaved inputs, while late callbacks cannot change an equal-coordinate
successor. These controlled UI tests complement existing core/FFI original-owner authorization tests.

Native receiving completion checks retain a recovery review prepared before setup finishes:
preparing another review after the pending link has been cleared is correctly refused. Publication
preview tests await their expected current row after the native draft read, before scrolling.
Activity recreation waits for the requested screen's current semantics after asynchronous protected
startup, rather than treating the Activity lifecycle acknowledgement as private-content readiness.
The profile recreation case requires its ready Profile label without dismissing a network wizard;
its original identity, wrapped-key and selected-slot assertions remain.
`AppLockStartupTests` restores saved private-content state with a distinct remembered startup owner,
while controlled read gates retain loading, protected-failure and retired-owner rejection coverage.
Lock-settings Back checks await the merged Account destination after both the UI button and native
Back dispatcher, with a fresh readiness deadline for each transition; transient unmerged nodes do
not establish completion.

Android native link journeys observe actual session completion through Compose `awaitSnapshot`.
Each transition retains its independent failure deadline and named stage; Compose drives queued
UI effects while native IO remains real. Blocking `Flow.first` on the instrumentation thread is
reserved for worker teardown after disposal. `ScreenReadinessTests` gates an actual LaunchedEffect
continuation to check scheduler progress without sleeps or guessed delays.
The instrumentation progress JSONL appends attempts to one file. Split records at each
`runner_started` before counting results or claiming first-attempt acceptance; a later retry's
success never replaces a primary failure, and an unfinished retry supplies no terminal result.
Desktop forwarding failure feedback sits after the selector beside its recovery actions.
After target rows render, the prototype explicitly reveals that recovery region, including when
Cancel already holds focus; repeated focus alone does not acknowledge the changed layout.
The keyboard plan checks their current viewport geometry together; calibration moves the alert
back above the long list to require rejection, then restores it. Controlled admitted completion
retains the existing Escape refusal and mounted-view acknowledgement before checking feedback.

Android receiving-screen checks use the real Settings frame, controls and retained-operation store
with controlled native completion. They preserve the exact reserved destination, require explicit
creation and disposition, and invoke captured application callbacks after disposal/equal reopening.
`ReceiveIdentifierTests` renders source/code/received/destination identifiers through those production
widgets in EN/RU at both text scales, checks real text density and viewport bounds, and verifies
fixed groups, raw accessibility text and exact system-clipboard values. The established link-code
copy tag stays stable while all identifiers reuse the shared display component.
`LinkReviewFailureTests` rejects a preview after all 31 explicit choices inside the actual receiving
Settings frame, acknowledges safe Back focus and the recovery relocation-completion signal, and
checks the complete error against the current viewport before any test-driven scroll.
Each recovery control is then exposed and checked with fresh bounds in EN/RU at both text scales. This covers preview refusal only; authoritative
confirmation refusal belongs to the separately retained enrollment state. Its typed notice and
existing recovery actions render beside the active form footer, or beside the reopen/saved state;
receipt-only outcomes keep their original notice. After the safe Back control reports actual focus,
the measured feedback container requests visibility using its full bounds, separately from the
action controls; focus alone is not a scroll completion signal. Changes to its measured size
retrigger the request without a guessed delay. A footer taller than its viewport need not fit
simultaneously: the full refusal is
checked first, followed by individual measured recovery-control reachability.
The phase-only `LinkReviewVisualTests` renders the actual complete comparison in the receiving
Settings frame: choices, preview, saving, typed refusal and unavailable photos, in EN/RU, both
colour themes and actual 100/200% text scales. With `comparisonVisual=1`, `sourceSha` and
`expectedWidth` instrumentation arguments it writes `files/comparison-visual/<width>/<language>`.
Each actual scroll acknowledgement saves a viewport; the scene can complete only when the saved
frames cover every laid-out line of every offered value. It is excluded from ordinary runtime.
External validation uses `tests/scripts/android_comparison_archive.py`: exact source, full matrix,
bounded archive paths, actual font density and union coverage of every required line must agree.
Photo frames separately expose each offered image or unavailable-photo retry control, with full
current viewport bounds; the validator requires those witnesses as well as text-line coverage.
Negative controls retain both edge frames while omitting middle lines or photo witnesses and
require rejection.
Use the manual workflow's `suite=comparison` to collect and validate this separate matrix.
`LinkReviewSaveRefusalTests` publishes each typed
native refusal before completing the admitted callback, retains all 31 choices, checks safe Back
focus and verifies that uncertain/unauthorized outcomes gain no ordinary confirmation or finish.
These checks establish presentation ownership; actual owner approval, enrollment, QR and runtime
acceptance require the separate real JNI journey and screen review. The receiving JNI journey also
remounts the production screen after installation: retired finish/open callbacks cannot act, the
exact original native receipt remains, and only the current control completes private-state import. Receiving prototype states run
`clients/android/prototype/receive-fixture-tests.js` after capture: typed refusal actions, code
visibility, activation prerequisites and busy disposition are checked before focus/reachability
calibration. Canonical Android capture validates layout and these browser contracts; it does not
run the desktop-only criteria audit or prove native transfer/enrollment.

Android `remove-offline` captures also run `remove-fixture-tests.js` after capture. It checks
that the translated uncertainty warning and all three actions are reachable using actual
scroll/focus completion, current bounds and hit testing, including enlarged text. Hidden,
covered and off-screen counterexamples calibrate the shared reachability check. Initial
viewport PNG images alone do not show the entire long dialog or prove native removal behavior.

`LinkEnrollmentNoticeTests` exercises the production enrollment notices and controls in EN/RU.
Uncertain authorization, private-state review, revoked and unlisted outcomes remain distinct and
expose neither completion retry nor review refresh, including after a comparison was offered.
Neighboring transient failures and field-review requests retain their respective explicit actions.
These controlled presentation checks do not renew authorization or prove native enrollment.

`ReceiveVisualTests` is a dedicated rendering phase for the 25 receiving/setup prototype neighbors.
It uses the production receiving content and Settings frame, with core-produced invitation QR and
real isolated native keys; outcomes are controlled and no poll, network or enrollment is performed.
The matrix covers EN/RU, light/dark and 100/200 percent fonts, checks current paragraph/scroll
geometry and retains top/bottom PNG frames. It is excluded from ordinary instrumentation. The manual
Android visual workflow runs it before the real-IME phase, with a deadline renewed only by completed
cases. It retains partial receiving archives and requires exactly 400 source-bound frames per width;
owner/IME namespaces, wrong widths, links and incomplete inventories are rejected. Successful
collection does not replace inspection of the actual frames; native visual acceptance remains open.

Comparison prototype captures also run `clients/android/prototype/review-fixture-tests.js`: all
31 offered groups retain full source values; initial comparisons have no checked answers; photo
reference disclosure does not select a source. Explicit choices survive read-only preview and return,
partial choices cannot save, confirmation admits one transition, and retired editable owners cannot
change an equal replacement. These controlled browser fixtures do not establish native profile
validation, photo availability, device authorization or persistence.

The native comparison form has independent Compose checks for complete EN/RU values at normal and
200% text, unsigned-code/income preservation, partial/coupled groups and current viewport geometry.
Its actual application callbacks are replayed after an equal fresh opening. Confirmation uses its
completion promise with no required busy frame; preview/edit focus follows composition, window and
control readiness. Controlled photo completion after form retirement cannot publish old pixels.
These presentation checks complement the real two-phone JNI journey, which prepares an actual core
JPEG, transfers its references and invokes the original review thumbnail binding before unchanged
separate enrollment, private-sync and revocation assertions. They do not substitute for full screen
review or all recovery-policy branches.

The owner approval dialog handles its own IME insets with an inset-aware window and `imePadding`;
its fixed action row must remain above the real keyboard while the body scrolls. The real-IME test
retains full glyph/action containment at each selection and the original per-transition deadlines.

Android stale-callback checks capture the installed `applicationClickKey` action, rather than a
Compose `SemanticsActions.OnClick` wrapper: modifier nodes may reuse that wrapper for a new opening
or require attachment before invoking it. Identity durability acknowledgement uses that same exact
callback contract for replacement results and disposal. Profile sharing exposes the same application action used
by each real control; cancellation, duplicate admission and retired completion assertions remain.
The real foreground LAN-permission journey hides the IME through its actual Activity window after
text replacement. `ActivityImeTransition` listens for pre-draw/window-focus callbacks and completes
only when current insets show no IME, the window has focus and layout is settled. Each invocation
has its own failure deadline and removes listeners even on failure. The test then scrolls to the
check button using current layout bounds before clicking. Espresso root selection is not the
readiness signal: on modern Android its focus prerequisite can block the IME action itself.

Received-message prototype actions use the existing spacing token separately from compact message
metadata. Empty received-menu holders are hidden; the normal/enlarged layout validator checks the
actual control separation without allowances.

Desktop forwarding adds nine families beyond the original 99-family matrix. Keyboard calibration
reopens the real static picker and requires a fresh owner, ignored retired choice and unbound old
pagination. The admitted-send frame checks disabled actions and Escape refusal, then acknowledges
a controlled failed completion before the normal GDK focus/radio/Escape-restoration audit. It never
cancels backend work or exempts the ready state from existing keyboard checks.

Native desktop Settings smoke captures the actual blocked and passed panels after their initial
load completes, before replacing their rows with pagination/error fixtures. These named capture
points run the existing text/accessibility checks on the synchronization hints in all desktop clients.
The focused `settings-moderation` step runs independently in EN/RU on all three native desktops.
Linux additionally retains the loaded panels at 440/1280 widths in a fresh `moderation-shots-*`
directory; a receipt binds all eight required PNG files to their hashes and the executable hash.
Missing/empty captures fail the exposure. CI uploads this directory with native diagnostics;
its Linux setup includes the same ImageMagick capture tool already used by the Docker test image.

Native desktop received-view capture cases exercise loading, populated, empty, initial-error,
retained-error, no-query-match, next-page and profile-return states in EN/RU, both themes and
measured 100/200-percent text. Linux retains all 128 required 440/1280 PNGs in a fresh
`received-views-shots-*` directory with source SHA, executable and image hashes. Other desktops run
the same actual-pane accessibility checks without the Linux capture tool. Held callbacks and
controller completion signals establish states; capture evidence still requires rendered review.

The prototype keyboard fixture calibrates requested switcher visibility against both the real
visible menu and a controlled hidden sidebar. It must reject all three missing identity actions;
frame readiness and current hit testing establish exposure without fixed positions or delays.
Calibration fixtures temporarily suspend prototype URL updates while retaining the real DOM/focus
transitions; WebKit limits rapid history writes independently of UI readiness. A 128-transition
counterexample checks that the URL stays unchanged and ordinary URL navigation resumes after a
controlled rejection. Fixture cleanup always releases this scope, including setup failures; normal
keyboard walks and prototype navigation keep their URL updates.

Offline native chat fixtures that establish several conversations must pass distinct interaction
event IDs per author. Reusing an author/event pair for a different peer correctly fails with
`InteractionConflict`; `establish_with_event` makes the fixture's distinct events explicit.

Android prototype snapshots include nine forwarding states. Their browser checks live beside the
prototype in `clients/android/prototype/forward-fixture-tests.js` and are not packaged in the APK.
The static controller lives with shared HTML-prototype assets at
`clients/desktop/prototype/forward-fixture.js`; Android browser tests remain beside Android.
The static forward opening owns abortable listeners, including collection navigation; retirement
removes those bindings. A controlled browser check requires navigation before abort and no work after it.
Desktop outcome-to-source navigation restores the original peer, header/avatar and presence visibility;
its reopening calibration checks both sent and queued outcomes before exercising retired callbacks.
Run the on-demand `prototype-review` workflow with `platform=android` for the complete Android
inventory and layout checks; its default `platform=desktop` retains desktop capture and criteria.
The artifact records the selected platform, exact source and input hashes. Android owner-approval
prototype checks preserve the original public owner/code, distinguish read-only preparation from
admitted sending, retain partial-delivery uncertainty after expiry and use the shared current-geometry
control reachability checks. These static outcomes do not prove native authorization or delivery.
Ordinary APK CI does
not execute the HTML-prototype capture. After capture, browser checks verify
state-specific admission, received-menu permissions and destination-only history/avatar. Each ready
dialog snapshot also checks the actual 32+8 pagination, full-ID search, retained selection across
pages and rejection of callbacks after a synchronous leave-and-return transition. These checks
use DOM actions and completion callbacks; they do not stage native messages. After capture, each
applicable Forward/Cancel/read-retry control must accept focus, scroll into the current viewport and
pass center hit testing with its full live bounds inside the viewport. Intersection/scroll/resize
signals and a fresh per-control failure deadline establish readiness. Controlled hidden, covered and
fixed-offscreen mutations calibrate rejection, then restore the original node and presentation before
selection/retirement checks. Synthetic `.click()` alone is not evidence of physical reachability. Native forwarding
and receiver-history evidence remain separate requirements. The registered-shell JNI forwarding test
reads the clickable chat header in the unmerged semantics tree, preserving its exact target assertion.
It observes either target readiness or the existing refusal notice; refusals expand that dialog's
native details immediately, and noncompletion retains scoped header/dialog text. The original
transition deadline and actual receiver checks remain. The production-dialog Compose test
also requires a controlled precommit send failure to retain its selected destination and draft,
then allows exactly one additional forwarding-call attempt after an explicit retry.

`ChatFormOwnershipTests` renders the production remembered form and replaces either its peer or
the actual opened identity while retaining equal slot/owner identifiers. It requires a fresh form
with no carried draft, edit, deletion, rename, confirmation or pending flags. Recomposition and
SideEffect completion, rather than a delay, establish the new owner before assertions.

Shared visible-chat admission tests keep the existing own/blocked/unknown error order, all known
conversation phases, exact owner/peer projection, no writes and propagated storage failure.
Native Clear-target tests cross the public export, reopen its store and recheck refusal after the
peer is blocked; the earlier target record never authorizes a later Clear. These tests do not
replace Android target/consequence rendering and stale-dialog callback acceptance.

Desktop and inherited prototype withdrawal warnings use the same unconfirmed wording as Android;
a missing acknowledgement cannot establish that no node accepted a remote write.
The native profile-removal journey checks the complete unconfirmed-withdrawal warning and its
text layout before exercising Retry/Cancel, then verifies normal removal after network recovery.
The separate Remove anyway path retains explicit local key/data-removal acceptance. Resource copy
must not promise a fixed remote expiry or claim that an unconfirmed completion proves no node
accepted the withdrawal.

The native linked-media protocol regression first establishes a matched chat containing a valid
JPEG and decoded WebP sticker, verifies both protected node blobs and recipient mixed-history
rows, and only then enrolls the extra device. The public native receiver must join, receive
subsequent text and reopen with no earlier photo/sticker rows or cached originals. Original
protected blobs are checked again after enrollment and reopening so data disappearance cannot
make the absence assertions pass. Existing linked-text, idle-revocation and third-own-group
protocol cases remain separate. The test requires core protocol CI; source checks alone do not
establish its runtime acceptance.

Localized Android Compose fixtures use `LocalizedTestContext`: provide the selected context,
configuration and `LocalResources` together, plus the desired current density/font scale.
Capture the real Activity's `LocalActivityResultRegistryOwner` before replacing its context and
provide that same registry inside; a configuration context alone is not an Activity registry owner.
Keep locale-specific text/description assertions and actual geometry checks; do not turn a missing
localized node into a timing retry or accept a different language. This helper also covers
localized full-screen fixtures that can open a system picker. Key its configured window host by
configuration values, font scale and parent window, rather than a caller's Context object identity:
fresh equivalent locale wrappers must not dispatch repeated configuration changes. The native
regression retains the actual configured host and dialog draft across those equivalent wrappers;
separate scale witnesses still require real font changes to reach platform Dialog windows.

`SearchIntentTests` verifies that admitted native conditions, employer and mutual choice remain
unchanged after every form field and offered list are edited. An admitted validation failure
remains a failure after later correction; a captured mutual choice still requires the actual
saved own gender and exact age. These snapshot tests do not establish callback/operation lifetime
safety. `SearchPresentationTests` controls backend readiness and completion independently,
including work that returns after cancellation. It checks duplicate Run admission, retained
Run after equal-looking failure, old Cancel against a successor, late offered-field installation
and retired-owner completion/admission. Assertions wait for the actual coroutine job completion,
not just a callback-internal finally block. Native lifetime and real query acceptance remain
separate from this presentation-controller scope.

Checked shared-search tests match canonical Unicode interest conditions against actual profile
tags, preserve punctuation, reject invalid nonblank offered tags without a partial query, and
ignore invalid hidden fields. Native adapter tests retain condition ordering/payloads and map
only offered invalid interest input to the field-specific failure. Android query/UI acceptance
requires device execution. `SearchInterestTests` crosses generated JNI for canonical Unicode,
punctuation, offered-invalid refusal and blank/hidden omission, and renders the actual filter form
to verify offered visibility changes retain raw input without applying hidden conditions.
`NativeInterestSearchTests` publishes actual slot-owned profiles through the existing suite node,
obtains its offered fields and queries a unique canonical tag using decomposed/uppercase input.
The verified result must contain only the expected peer and its complete saved tag list. A distinct
nonmatching tag is accepted as empty only after a complete, nonpartial, failure-free native search;
expired deadlines never establish absence. Each transition advances on a real completed search.
This test requires device CI and the existing suite node; it does not make Android a network node.

Shared multiline-interest tests cover CRLF/blank lines, punctuation, canonical Unicode, preserved
duplicates, exact count limits and invalid-tag refusal without partial output. Native adapter tests
compare advertised limits against actual parser admission at each boundary and distinguish clear
from invalid input. Native parsing alone does not establish Android editor/persistence/search acceptance.
`NativeProfileInterestDraftTests` checks optional JNI record roundtrips, unrelated-save preservation,
explicit clear after reopen and another owner's retained tags. `ProfileInterestEditorTests` checks
native canonicalization, blank clear, untouched-field omission and whole-save refusal. Its controlled
late-save case retains newer input and then persists an explicit undo of the completed older write.
`ProfileInterestUiTests` exercises actual editor save/reopen/clear and invalid-input recovery through
MainActivity. `ProfileInterestRowsTests` exercises the actual shared public/preview rows, preserving
punctuation and duplicate order and distinguishing an interest-only profile from an empty one.
`InterestGeometryTests` renders the actual editor/search fields and public rows in the production
theme at EN/RU normal/doubled font scale. It checks full labels, native-limit hints, a long multiline
input and public tag list using current text layouts and viewport bounds. Device CI execution is
required; component geometry does not replace full screen/keyboard/owner review or network search
acceptance.

Native privacy preference tests use real offline ChatSession/Store fixtures for default values,
per-field independence, reopen durability and another identity's defaults. Unsupported flag formats,
invalid values/lengths and a database-triggered write refusal must propagate errors without
repairing the stored value or changing adjacent preferences. Generated bindings, Android controls
and real governed signal delivery require their own CI/runtime evidence.
`PrivacyPreferenceStateTests` covers unknown/read-retry, pending duplicate admission, unconfirmed
write/readback, disposed-owner completion and equal fresh read revisions. Presence uses this shared
presentation guard; its existing controlled Compose scenarios retain their original assertions.
`PrivacyPreferenceSwitchTests` invokes the actual application callback to test unknown views and
read-only retry, committed-but-unconfirmed receipts, and pending typing with owner replacement.
Counters are checked outside caught callbacks; controlled completion signals avoid guessed delays.
The existing production-Activity receipt persistence/recreation test remains unchanged.
`NativePrivacyPreferenceTests` crosses the actual registry/Keystore/generated JNI boundary through
production preference adapters and an independent direct native field snapshot, so a paired
getter/setter swap cannot pass. Independent changes preserve presence/notices, another selected
identity retains its defaults/selection, and reopening restores the edited values. Each native
read/write gets its own failure deadline; no node or Activity is needed. Device CI must execute it.
`NativePresenceDeliveryTests` separately uses two actual clients on the suite-owned node: mutual likes,
hidden sender refusal before first disclosure, explicit send and authenticated receive, retained
incoming marks while hidden, no automatic reciprocity from receive, and explicit reverse send.
Receipt advances only on completed native mailbox passes; common injected-clock tests retain
throttle/expiry boundary coverage. Foreground-lifecycle and rendered UI acceptance remain separate.
`PrivacyPreferenceGeometryTests` renders the actual three preference components in the production
theme at EN/RU normal/doubled font scale. It measures complete labels/hints against live row and
viewport bounds, then controlled read failures and retry-button text/bounds. Every retry must
restore a known enabled row without any write call. Component geometry requires device CI;
full Settings navigation/Activity review remains a separate acceptance scope.

Debug-only `MailNoticeProbeService` observes actual framework start/stop/completion and attempts
at OS notification posting without replacing registry selection, native receive or posting.
The attempt carries the original admitted slot/peer and a notification copy, after foreground and
OS-permission gates. Observers must only record signals/data: they run inside native ownership and
must not reenter RustBridge. A zero attempt count is meaningful only after verified job completion;
an expired deadline never proves that no notification was attempted. Actual OS-post integration
acceptance remains separate from these observer hooks.
`WorkerStickerNoticeTests` uses owned registry slots and an independent native sender on the suite's
real node. It registers an accessibility notification-event listener before scheduling the debug
service, checks the actual private/public OS payload in Text/Name/Fixed modes and original slot/peer,
then verifies exact sticker receipt without saving a pack or retaining an image original.
Muted and notification-off cases require native job completion, zero observed post attempts and
the exact received sticker; an absence timeout cannot pass them. After the recipient withdraws its
like, the still-unaware sender submits a sticker accepted by the node; the completed worker must
keep the chat non-open, reject that exact sticker and attempt no notification or media retention.
Unlike an eligible text request (which may produce a fixed notice), non-open sticker bodies are
acked and dropped by the core; the core receive tests separately assert inbox consumption.
The fixture restores the previous identity inventory. It requires CI/device execution, notification
permission and no live Activities; periodic/Doze scheduling remains separate. Cache/no-save checks
prove no persistent media side effects; they do not measure network requests.

Shared own-message preflight tests retain desktop full-key/live-text/delete-only sticker cases
and real Store coverage for colliding authors, media kinds, blocking and cleared history. The
original desktop regression assertions remain and their argument helpers borrow items for the
common matcher, including all original author/group/ID/kind counterexamples.
Native sticker-delete boundary tests check exact selection and no writes before transport,
including a valid own selector reaching an intentionally absent network and unchanged text-only
compatibility. `NativeStickerExchangeTests` reuses its real peer pair for own deletion, foreign and
duplicate refusal, remote removal after native receive completion, and retained original sticker/packs.
`StickerDeletePresentationTests` controls Cancel/reopen tokens, duplicate admission, precommit refusal
and noncancellable completion after retirement. `StickerDeleteUiTests` drives actual own/foreign
history rows, the Delete-only menu, Cancel/reopen and identity retirement, exact selected keys and
committed duplicate rejection with an unchanged composer draft. `StickerDeleteGeometryTests`
renders that same history/confirmation with long literal pack names in EN/RU at normal/doubled
font scale, scrolls the complete explanatory text, and checks full action labels and live dialog
bounds. Cancel must leave both the delete-call counter and composer unchanged. Actual JNI/UI
execution remains required CI proof.

Common received-pack control table tests cover current phases and failures, both retained and
absent generations for failures, exact order and unchanged controls across zero/maximum retry deadlines.
The desktop controller test deliberately supplies controls that disagree with its former state
rule, retires an old button and rejects an unknown control while retaining the known pending
caption with read-only recovery; all original quota/fresh/conflict/focus
assertions remain. Native request-status integration tests also check controls on actual New/Waiting/Downloading/
Fetch-failed/Saved transitions. These require native CI; formatting alone does not prove execution.

Sticker request/status and manager/page snapshots use referential state policy because their
object identity admits callbacks. An equal fresh native value must replace the old object;
`SavedStickerSnapshotTests` verifies that an old page callback cannot trigger another read.

Android `SavedStickerPresentationTests` controls cancel/duplicate removal, uncertain unlink
readback, retired picker tickets and closing during noncancellable URI reads. Confirmation tokens
are unique per opening: a callback captured before Cancel cannot confirm a reopened dialog for the
same entry. Import admission
occurs only after the owning opening observes read completion; known pages remain visible but
not editable when readback fails. The native adapter uses registry-owned removal without opening
a media key. The reusable manager dialog shows bounded pages and a selectable full-ID removal
confirmation. `SavedStickerManagerViewTests` checks same-prefix IDs, Cancel/reopen retirement,
exact core cursors and retired page callbacks via semantic completion signals. The production
shell hosts Settings → Photos and a common manager route with per-opening picker registration.
`SavedStickerManagerHostTests` drives the real OpenDocument registry contract, Android file-stream
reading and native byte-budget export with controlled results: old/new openings have separate
request codes, and only the current file bytes may reach the save adapter. It also checks retired
identity navigation. `SavedStickerGeometryTests` reuses the actual manager fixture in the production
theme, with long literal Ready names and unreadable entries at EN/RU normal/doubled text scale.
It scrolls full explanatory text and the selectable complete PackID, checks current dialog-relative
action geometry, and requires Cancel to preserve the selected pack with zero remove calls.
Native TAR validation/storage and actual rendered/device execution remain separate CI proof.

Android `PickedInputTests` checks empty/exact limits, fragmented provider reads, partial-read
failure propagation and oversized input rejection after at most one extra byte. The OS reader
uses core-provided budgets; it performs no archive, image or quota validation. Photos reuse this
reader with their existing native budget and localized too-large/unsupported errors.

Android `StickerPackPresentationTests` controls native-like request completion and readback:
uncertain application followed by failed read retains the known intent and disables mutation;
read-only retry installs the current generation; retired/same-token status callbacks and duplicate
admission cannot submit. Explicit completion gates have separate failure deadlines. The request
presentation controller and Compose controls are wired through production ChatFeed and the shared
saved-pack manager. `ChatStickerPackRouteTests` drives chat-to-manager navigation, close/readback and
mailbox-revision status replacement. `StickerPackControlsTests` checks authoritative projected actions,
retired-owner navigation, pending caption retention and read-only recovery. Button coroutines
start undispatched to admit an operation before the next captured callback can run.
Actual instrumentation execution is required independently of source lint.
`SavedStickerStateGeometryTests` drives the production saved-pack manager in EN/RU at normal/doubled
text, holding read/import completion explicitly. Busy controls, retained-page read failure and
read-only recovery, added/already-saved and failed import notices use current layout/viewport bounds
and external attempt counters. The controlled import bytes prove presentation only; real TAR and
quota admission remain native-core/host tests.
`ProfileShareOpeningTests` retains actual dialog callbacks across Cancel and equal-content reopening.
Only the current opening admits a send once; retired Cancel, Open Chat and completion callbacks
cannot alter a successor or unlock its pending request. Successful/queued commitment admits only
navigation to its exact destination, while current refusal permits an explicit retry.
`ChatPhotoGeometryTests` exercises the production photo bubble in EN/RU at normal/doubled font
scale: explicit stranger reveal, held download, failure/retry and actual PNG decoding. Completion
signals and external request counters separate phases; complete label layouts, current viewport
bounds and decoded image aspect are checked after scrolling. This presentation fixture does not
replace the independent native photo transport/cache tests.
`StickerPackGeometryTests` renders the production request widget in EN/RU at normal/doubled font
scale with long literal pack names. A controlled held read checks the disabled checking presentation
before releasing it and awaiting loaded UI completion. Loaded cases check complete status/warning
layouts and current width, and scrolls actions fully into the current viewport. A supplied full
control list exercises every label without duplicating native eligibility; failed readback retains
the known caption and leaves only read-only recovery. External write counters stay zero.

Android `StickerPickerPresentationTests` controls equal-content choice replacement, duplicate and
late sends, typed Queued commitment, native cursors and Back through a failed read.
`StickerPickerNavigationTests` covers exact-opening/manager-revision return and a chat that stopped
allowing composition. `StickerPickerComposerTests` uses actual `MediaChatContent`, editable/held
text drafts and the real saved-manager route with controlled native projections: sticker send stays
standalone, old buttons cannot send after manager return or identity replacement.
`StickerPickerGeometryTests` measures wide/tall thumbnails, full labels and current dialog bounds
in EN/RU at normal/doubled font scale. It scrolls semantic targets and uses live bounds, without
coordinate guesses. These require SDK/device CI; the real-node native sticker scenario separately
proves backend pack authentication, send, recents and hidden controls.

Android `ChatStickerImageTests` checks actual Bitmap alpha/channel conversion, rendered aspect and
localized accessibility labels. Controlled native-like completion gates cover duplicate manual
requests, replacement owners, retired callbacks and failure/retry. Both old and successor reads
have external call-count assertions, so caught callback failures cannot hide redirected work. These tests retain complete
message keys and observe semantic outcomes with per-transition deadlines; SDK/device execution
is required separately from strict source lint. Pack-management and background-notice acceptance
remain separate required slices.

Android `SlotsTests` checks fail-closed presentation and projection labels; removal lifecycle
eligibility cases live in the common `identity_slots::removal_policy` table test. Core registry
review tests retain a real shared lock during key reads and cover denied/missing keys, stale Peer
IDs, duplicate-key warnings and unchanged selection/files. The exported registry fixture and
`NativeSlotsTests` cross the actual FFI/Keystore boundary for ready and cleanup hints and wrong-peer
review refusal. Their SDK/device execution is required in CI; standalone pure-policy/conversion
checks do not prove registry locking or native binding generation.

`RenameDialogGeometryTests` renders the native private-name dialog in EN/RU at normal and doubled
font scale, exposes every label/hint line using its measured bounds and checks complete layouts/current
dialog bounds. Floating-label clearance follows its actual measured height. The shared native geometry
helper selects a vertical scroll ancestor, skipping single-line inputs' horizontal scrollers, and waits
for each line's visibility with a fresh failure deadline. Save/Cancel remain outside the scrollable fields; cancellation invokes no save,
and an empty name still reaches the save callback. Device execution is required separately from
source lint and WebKit prototype captures.

`LocalCollectionViewTests` exercises the actual shared-core FFI projection from Compose: 70 local
entries mount as 32/32/6 rows, full-target search finds a last-page entry, clearing search returns
to the first page, and a captured navigation callback cannot change a replacement opening.
Controls scroll to their current bounds; row readiness uses semantic completion signals. These
test sources require native CI execution; source lint does not establish device success.
`CollectionRowGeometryTests` renders the production identity/people row controls in EN/RU at
normal and doubled font scale. It checks complete identifier/action text layouts, actual current
horizontal containment and action placement below the identifier, without fixed tap coordinates
or guessed row heights. Execution on Android remains required.
`IdentityCollectionStateTests` controls read admission/completion with a future and per-transition
deadlines: failures preserve the known snapshot, successful retry replaces it, concurrent refresh
admits one read, and a retired owner's completion or selection cannot publish. Existing
`IdentitySettingsTests` still checks confirmed active/non-active removal through the real registry;
its non-active row lookup now uses the complete displayed Peer ID.

`LanguagePreferenceTests` covers arbitrary catalogue size, legacy enum-name matching and explicit
system/removed-choice fallback. `LanguageCatalogTests` reads the actual packaged XML inventory and
checks canonical tags, readable autonyms and exclusion of generated pseudo-locales. Python
`test_android_language_catalog.py` compares registry tags with own application translations,
including region/script qualifiers while excluding UI/night/density qualifiers and libraries.
Kotlin test sources still require actual SDK/unit/device execution; Python inventory tests alone
do not prove native resource selection or recreation.
`LanguagePickerTests` checks Cancel/unchanged selection without writes, stale callbacks from an
older opening or owner, arbitrary option count and single admission of captured repeated selection.
`LanguagePickerGeometryTests` checks complete long-label text and current unclipped dialog bounds
at normal/doubled font scale, with Cancel still visible. Its configured native window and measured
label TextLayoutResult must agree on the requested fontScale. Initial/return focus readiness uses actual
window-focus signals. The selected option and returning opener explicitly allow focus in touch mode.
Retirement tests capture the same application closure supplied to the control through
`applicationClickKey`; Foundation's retained `OnClick` wrapper requires an attached Modifier node
and cannot represent delivery after disposal. Ordinary interaction tests still click the real control.
`AppearanceTests` enters the dialog and preserves its existing native
language/theme persistence and Activity recreation assertions. Actual CI execution remains required.

`ChatListPreviewTests` drives actual conversation rows for all six typed bodies and forwarded states,
with disagreeing legacy text/time, cached shared-profile labels, original navigation and a full-history
sentinel. `ChatListPreviewGeometryTests` reads actual semantic text layouts and unclipped bounds at
normal/doubled font scale and asserts the preview's actual layout fontScale; its inline density
provider needs no separate Dialog window. Its narrow host derives from measured text and current width.
It requires wrapping, complete non-ellipsized content, timestamps below the preview and contained
horizontal bounds, without guessed offsets or sleeps. These sources require actual SDK/device CI;
static lint alone does not establish rendered acceptance.
`ContactNameSnapshotTests` separately holds a replacement read pending and failing, completes a
retired noncancellable read late, and fails a same-owner refresh. Each transition observes its
completion signal; replacement snapshots must stay empty/current, while refresh may retain only
that owner's last successful local names.

`MediaCacheErrorTests` controls unknown-read retry without default writes, raw invalid input and
indeterminate-write readback with draft retention. `MediaCacheOwnerTests` checks cancellation without
a confirmed mutation, a cancelled dialog callback versus a newer opening, repeated captured Confirm
admission, and late noncancellable completion after identity replacement. Gates are released in
cleanup even if an assertion fails; transitions use actual completion/readiness signals.
The widget leaves parsing, cache policy and confirmation decisions to common core through UniFFI.
`NativeMediaCacheTests` separately exercises real JNI exact input, confirmation, preservation of
independent fields, reopened identity isolation and actual cache clearing without losing private
notes/favorites. Only actual instrumented execution establishes this native acceptance.

Shared cache input tests cover integral-byte decimal/exponent conversion, exact non-whole MiB
round trips, range/overflow refusal and destructive-change boundaries. Store-control tests require
confirmation before mutation, reevaluate actual state and preserve the other preference and
unrelated data. Native export tests check typed invalid input, confirmation, restart and identity
isolation; these cases require actual CI execution rather than binding/lint-only acceptance.

`SettingsHubTests` checks the native category root has no collection/removal actions, detail panes
hide shell bars, collection Back returns to its category, and system Back restores the root.
Existing settings tests enter explicit categories and await native values/rows after navigation.
After Activity recreation, the helper enters Settings before returning a restored detail route to
its category root; tests do not assume that selecting the tab resets its remembered route.
`SettingsFrameOwnerTests` captures an old collection callback, replaces its identity and invokes
that callback to prove it cannot navigate the successor. The route-only owner survives the app-lock
settings overlay without retaining native handles or loaded values.

`PhotoDownloadOwnerTests` controls the real three-choice native-settings presentation. Unknown reads
leave every choice disabled and retry performs no write; repeated captured callbacks admit one
change, and late completion of an old native-like operation cannot replace the next identity's
selection. A write that commits and then fails disables choices until read-only retry reconciles
the actual value. The widget delegates network/stranger policy and persistence to the existing core API.

`ContactNoteOwnerTests` checks exact multiline/non-BMP draft retention after controlled save failure,
retry and clear, unknown-note read retry without editable/Save controls, and an old noncancellable
native-like completion after identity replacement. Signals order each transition with separate
failure deadlines; no arbitrary delays or coordinate estimates are used. Native privacy, blocked
contact access and persistence remain covered separately by `NativePrivateContactTests`.

Android `FavoriteActionOwnerTests` drives the real private-save control with controlled native-call
completions: two captured callbacks in one UI turn admit one write; successful readback determines
the label; replacing an owner cancels its prior presentation; failed initial reads offer only read
retry before any mutation. The existing JNI private-contact tests separately cover durable native
state, typed policy failures and absence of public-profile/like/chat changes.
`SavedProfilesTests` separately checks lazy local-list reads, navigation only after an explicit row
press, captured Open callbacks during removal and after the row disappears, and cancellation on
identity replacement. Only the ordinary profile route performs the later network fetch; expanding,
refreshing or removing a saved row uses local native operations.

The bootstrap protocol fixture `native_linked_receiver_joins_existing_chat_without_old_history`
opens a linked device through public `ChatSession` calls against a real external node. Its existing
pair chat contains pre-enrollment traffic; after completed membership maintenance and native mailbox
processing, the new device has no previous messages and receives only subsequent traffic. The
sender's maintenance uses an explicit clock; protocol completions order the phases without sleeps.
The companion `native_idle_refresh_removes_a_revoked_contact_without_new_mail` reopens the existing
sender through FFI, publishes a contact device removal, and checks the durable MLS members before
and after ordinary receive without new incoming traffic or explicit confirmation. These fixtures
cover native receiving/maintenance adapters, not Android UI. The separate common protocol fixture
`a_third_own_device_reuses_the_private_group_and_syncs_both_directions` publishes a third own device,
runs maintenance on both existing members at the same injected clock, and receives the Welcome before
the new device performs maintenance. It checks the original group ID, bidirectional private changes
and absence of chat rows; this common-core proof does not imply three-device JNI execution.

The shipped desktop sticker-controller paging check awaits its actual listing completion, then
checks the localized Pack ID label, contiguous eight-digit prefix and accessible Remove target.
The full removal confirmation retains the complete identifier. Controller readiness and focus
come from completed callbacks and the current DOM, with no timing or positional assumptions.

The iOS typed-profile journey awaits editable/hittable field readiness and measured containment
above the current keyboard. A partially exposed hittable field is insufficient for native entry.
It then performs native text entry,
and then observes the exact seeking-age value in that field. It does not gate typing on a KVC focus
flag; native entry failures and wrong-target values remain failures. Existing invalid-height,
persistence, preview and relaunch checks stay part of the same journey.

Android v0.0.1 background acceptance retains exact-message ownership, offline reopen, reboot and
APK-upgrade/job-restoration phases. Doze/App Standby policy verification is planned after v0.0.1;
forced worker dispatch does not prove natural idle or periodic execution. Reopening that work
requires owner approval and a fresh cost analysis.

`NetworkPermissionOwnerTests` queues foreground permission continuations around the actual
`NetworkDraftScreen` and two native identities. It replaces and disposes the original form before
completion, verifies unchanged selection/active settings, and completes a current request as a
positive control. Invalid-address results avoid network timing. This controls callback delivery;
real API 37 permission-controller dialogs remain separate device acceptance.

The modern permission lane also runs `LocalNetworkDialogTests` after restored-profile cleanup.
AndroidX Test `SdkSuppress(minSdkVersion = 37)` gates the test class; setup separately requires target
API 37. Setup and restoration retain checked synchronous preference commits, not unchecked KTX writes.
It revokes permission and clears decision flags before starting the instrumentation process, then
restores the grant even if the foreground test fails. The actual `MainActivity` network form first
requires a denied request to complete with a native invalid-address result. A second OS request stays
pending while the activity is recreated; lifecycle callbacks acknowledge old destruction and new
creation before permission is allowed. Its retired continuation must leave the persisted draft and
replacement form untouched. A fresh request after the grant must complete normally, with the original
selection and active network preserved. This uses direct main-thread recreation because
`ActivityScenario.recreate()` first requires RESUMED, which a pending system dialog can prevent.
Each edit and check-button tap
scrolls its target into the current viewport; completion requires the result row for that attempt's
address as well as the invalid-address summary. The one-shot auto-prompt marker
is set before activity launch and restored afterward, so startup cannot race a system dialog. `PermissionDialog` observes
accessibility events and resource IDs scoped to the current system-owned window with fresh
transition deadlines. It verifies that window's package through public PackageManager metadata;
it matches the observed resource entry within that owner window rather than assuming the APK
package is also the resource namespace. Traversal is bounded, ambiguous controls fail, and failures
include the observed package/flags/resource IDs without copying dialog text. It uses no translated
labels or guessed geometry. The retained report is
`local-network-foreground.txt`. Compilation alone does not establish actual dialog execution.

Android committed identity persistence uncertainty is acknowledged separately from operation failure.
`IdentityDurabilityTests` checks EN/RU/pseudo normal/enlarged dialog text, live-region semantics and current
acknowledgement bounds, selected-route preservation and retired callbacks across equal replacement
results. `IdentityActionsTests` retains committed selection, clears the notice on a synced successor
and refuses a cancelled owner's unconfirmed completion. These tests use completion signals.
The Android prototype inventory includes `identity-durability-dialog`; its awaited post-capture
fixture checks acknowledgement-only navigation, polite notice semantics and the shared focus,
scroll and hit-test calibrations, preserving the static screenshot.

Android publication callbacks admit one attempt synchronously before coroutine dispatch. Closed,
busy and disposed owners cannot launch another attempt; native authorization remains unchanged.
`PublicationAdmissionTests` invokes captured callbacks in one UI turn and gates completion with
signals, including disposal while the attempt is suspended. Edit/save cancellation remains covered
by `PublicationBoundaryTests`.

Profile facts use bounded label/value tracks and wrap long labels; safety actions wrap within the
current panel. Contrast retries align the actual text start inside its scroller, then restore scroll
and focus. The scroll calibration checks readable and deliberately low-contrast text both shorter
and taller than its container, deriving the taller paragraph from current geometry.
Opaque ARIA-modal contrast isolation also works when every action is disabled and focus stays
outside. Its calibration retains readable and low-contrast controls and verifies that auditing
restores the original focus and background visibility.

The desktop report-path regression uses a fake executable callback that rejects explicit broken
and unknown-step arguments. It checks that the report contains every actual invocation and both
negative controls; it has no positional process-outcome queue or fixed case count. Runner tests
load both wrapper helper paths themselves, so isolated discovery does not depend on other test
modules having modified the import path first.

The common native sticker download regression gates a real libp2p MediaGet response. It changes
the same fixture's durable Block/Clear state only after observing the request, then releases the
response and checks refusal without cached pixels. An unchanged-state case checks exact decoded
pixels and actual cache retention. Each listener/request/completion transition has a fresh failure
deadline; the fixture never uses sleeps or a production fetch hook.

Android profile-share navigation targets the success dialog action by
`profile-share-open-chat`, then awaits the chat route and loaded composer as separate transitions.
The two-peer journey retains bounded active tags/bounds and delivery details on failure.
`ProfileShareDialogTests` physically clicks that action with a same-label background button,
covering Accepted and Queued outcomes, exact destination, dialog close and no second send.

Swift `ProfileDraftStoreTests` exercise raw profile inputs through the real shared parsers:
optional and impossible birth parts, height bounds, numeric overflow, seeking normalization,
unknown gender refusal and clearing disabled malformed fields. These deterministic input cases
need native iOS CI; Swift formatting/lint alone does not prove binding execution.

`RevealGeometryTests` checks form-scroll planning against actual label rectangles, viewport edges,
keyboard-reduced space and partially covered or oversized rows. These pure geometry cases run in
native iOS CI. `reveal` accepts native hit-test readiness before planning a scroll, including
partially visible rows. Fully visible controls await readiness instead of unnecessary dragging.
For each completed native drag it recalculates the viewport and noneditable accessibility-label
anchors, resets its failure deadline only after changed geometry, and
rejects repeated/no-progress states. Switch helpers reveal the actual native control separately
from its row label, await its enabled and hittable state, tap once, and observe the required value;
publication never blindly toggles an
already-confirmed value. All native iOS UI journeys stop at their first failed assertion;
subsequent interactions cannot turn failed setup into a long chain of unrelated failures.
Native execution remains CI-only; Swift lint does not establish simulator behaviour.

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
clean process exit. Windows uses `tests/scripts/production-windows-smoke.ps1`: UIA window,
top-level window events wake startup; structure and relevant property events on the owned window
wake rendered-control readiness, and process exit also signals the wait. Avoid subscribing to
unrelated desktop subtrees. The original failure is flushed before provider diagnostics.
Native-window and rendered-setup transitions have separate monotonic failure deadlines; unrelated
events do not extend them. Readiness observers are detached before Close destroys the app's UIA
provider; shutdown uses the process completion signal and still requires a clean exit within its
own deadline. Flushed phase markers distinguish observer initialization, process launch, window
readiness, rendering, observer release, close, process completion and exit-code verification. If the child PowerShell host dies, the parent retains its
failure code and reports at most five matching recent Windows crash events with bounded messages;
absence of such events is diagnostic only, never success. A failure records the process/window and a bounded UIA tree, while
retaining localized visible/enabled controls, missing-label countercheck and clean Close/exit.
The artifact check rejects missing, empty or smoke-instrumented binaries.
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
Every native lane runs the explicit offline `safety-number` case after reopening its saved
profile, including lanes without a separate node binary. A local own-profile card fixture drives
real safety UI, native QR and clipboard commands; it does not establish online search or peer
authentication coverage. Geometry checks cover hidden status, visible status and QR preview.
Unknown non-empty smoke steps fail explicitly; the runner checks that rejection separately.
Linux, Windows and macOS CI retain `desktop-ui-results.json` with named case results alongside
the captured diagnostic output; a green job alone does not establish coverage of a named case.
Native keyboard checks request native focus when inactive, then wait for actual document focus
through the window focus event and a deadline;
restoring a desktop window also requests native focus. Notification click scenarios await native
hidden readiness before receiving mail and actual notification-click restoration afterwards.
A hidden GTK window can retain its active flag; notification suppression requires both visibility
and focus, so these scenarios do not await an unrelated active-flag reset.
Opaque modal accessibility audits temporarily
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

Notification settings smoke clicks each `name`, `fixed` and `text` radio choice, waits for the
save completion, then reads settings back before asserting persistence. Restart coverage
checks that disabling notices retains the selected choice; stale-profile read/write guards remain
covered by controlled completion callbacks.
Android `NoticeCardOwnerTests` controls read/write completion on the actual card with isolated native
identities. It checks single-save admission (including a captured old action), disabled pending
controls, failure recovery, retained content when turned off, and cancellation of presentation
continuations after identity replacement or disposal. Each transition awaits semantics or an explicit
completion signal with its own failure deadline. Native `IdentitySettingsTests` covers all four stored
modes and off/content retention across recreation; controlled adapters do not substitute for that JNI
persistence acceptance.

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
control and its handlers for a later current-identity row. Collection smoke also checks explicit
table/row-group/row/cell roles after responsive layout activation. Controlled reload callbacks
exercise both success and rejection from an old identity while the current refresh is pending;
each start, completion and cleanup has its own failure deadline. Dialog readiness
requires both visible and open state, not only the hidden attribute.
The removal close handler ignores an old queued event after a newer opening becomes active.
Sticker removal waits for an explicit post-removal list-request signal before asserting loading,
then releases that held response before checking completion. The admission step has its own
named failure deadline through `timedChecks`; it does not assume a microtask count.
Publication checks inspect the dialog after the expected result and await its close event if
still pending. That fresh close deadline starts after proof/publication, replacing a fixed delay.
Disconnected-search checks require Open Network without Retry, then exercise generic, offline
and partial failures through completed production `runSearch` calls to catch stale recovery actions.
Desktop foundation smoke renders all six core chat-preview kinds with and without the
forwarded label. It supplies deliberately different legacy message text/time and requires the
core payload and timestamp, unchanged history data, no visual ellipsis and the original opening
callback. These cases are embedded in the native smoke inventory.
New-device acknowledgement smoke holds old and current callbacks across identity, peer and same-peer
reopening, for both success and refusal. It checks duplicate admission and that an old completion
cannot enable the current pending button, hide its notice or move focus; the current completion
still settles its own view. These six cases await actual callbacks rather than guessed delays.
Clear confirmation checks require both localized consequence paragraphs in order after the target,
while retaining Cancel's initial focus, return to opener and cleanup checks.
Android prototype geometry probes use WebKit text-only zoom for fixed-pixel typography and verify
that actual glyph lines enlarge while the viewport stays fixed; changing the root font size alone
is insufficient. Check text against its own layout block as well as the viewport and clipping
ancestors: a visible word can still escape its dialog surface.
Prototype first-message and received-request variants also audit the complete short Peer ID
and notice action labels at narrow widths and enlarged text. Their header reflow must preserve
ordinary chat, list, menu and dialog neighbors; existing request smoke retains the text-reply,
media restriction and exact Block target contracts.
Native desktop message-projection tests cover IDN/userinfo hosts, edits and deletion; the UI
link-host smoke consumes that IPC shape and retains the caption/no-preview assertions. JavaScript
must not reconstruct destinations from message text.
Peer-photo checks require no thumbnail row for a single image, and distinguish accessible names
for decoded thumbnails from letter fallbacks. Contact-renaming failure checks retain the exact
rejected editable text alongside its localized error. Chat-photo send checks retain an existing
text draft after successful sending and each tested refusal, including unsupported files.
Chat-photo rendering tests observe DOM completion for manual originals. A fixture-scoped wrapper
awaits the actual automatic-policy Promise, preserving its arguments and return value and restoring
the controller in `finally`. Opening and policy completion have separate named failure deadlines;
policy refusal requires no polling interval or extra task delay.

The mixed-profile node fixture writes its LF-only network configuration as bytes on every OS;
its regression intercepts the next native launch while simulating Windows text translation.

Photo controls use labelled vector icons. Avatar tests require fallback letters to stay out of
paint while a canvas is present and reappear after removal or decode failure; own and peer
avatars share that rendering contract.

Native identifier reflow checks count overlapping vertical bands of nonempty text rectangles,
not DOM fragment rectangles: chunk spans and WBRs can produce many rectangles on one line.
The check waits for fonts, retains narrow/wide wrapping assertions and rejects horizontal clipping.
The shared `installIdentifiers(strings, targetCode)` can observe a single code element; the default
invocation still observes the document body. Native smoke checks initial and replacement formatting
with detached siblings, so the body's default observer cannot mask a scope error or change the
outside sibling. Each replacement awaits its actual mutation signal with a fresh failure deadline.
The smoke also replaces a short one-chunk ID with the same text: formatting must restore the chunk
even when the replacement has the same raw value and child count. Cache reuse requires the same
child nodes, since node counts alone cannot distinguish a formatted chunk from a new text node.
Settings return helpers scroll the actual Back control into view before clicking, then await each
parent route. `SettingsBackReadinessTests` starts collection pages at their end with content sized
from the current viewport and requires Back to begin above the viewport; the helper must restore
root through both route levels. Network draft persistence invokes the enabled Save control's real
semantic action, then waits for saved status before recreating the Activity; moving IME/pointer
geometry is covered separately from persistence.

Interest editor/search regressions compare the exact `EditableText` semantic value. A labelled
Material text field also merges its title and supporting hint into `Text`; treating that combined
list as the editable value rejects correct input. Keep separate label/hint geometry checks and
save/reopen/validation assertions.

`ChatMessageSelectionTests` checks different-author collisions, exact lookup in either row order,
regenerated binding records and refusal of old incomplete selectors. `ChatSelectionRestorationTests`
restores actual saveable form state within the same owner opening and requires both edit/delete keys
to select the same full native author/group/ID. The existing missing-edit dispatch regression retains
its empty-history case and also tests the legacy key against a colliding two-author history: no task,
sending/status change or draft loss is permitted. Activity replacement with a new owner remains a
separate lifecycle contract.

`NativeForwardUiTests` prepares two conversations through the actual registered application owner and
the suite's external node/TestPeer clients. Activity recreation reloads durable rows without a guessed
mail-poll delay; the real shell then exercises received-text Forward, Cancel, exact full-ID selection,
confirmation and loaded destination navigation. The receiver must store the app identity as forwarder
with matching group/message IDs. The sender history and an isolated production `ChatContent` rendering
of the receiver's unmodified JNI history must show Forwarded. The latter is renderer coverage, not a
second registered Activity or OS-background proof. Native/Compose transitions retain existing deadlines;
selection/confirm use installed semantic actions to avoid IME pointer races. This scenario requires actual
Android CI execution before its delivery acceptance is closed.

`ChatForwardUiTests` mounts the production forwarding boundary, host, menu and dialog over a
controlled native seam. It checks received text has Forward without own edit/delete actions, searches
for an off-page full target ID, retains the complete native source selector, admits queued submission
once and preserves draft/held/edit state. Reopening an actual identity object with equal IDs retires
an old installed confirm callback. Real JNI/network delivery and destination-history feedback are
separate acceptance, not inferred from the controlled navigation callback.
`ChatForwardGeometryTests` uses the same host fixture with matching localized resources and normal/
doubled font scale. It scrolls actual selected/full-ID rows into view, checks complete measured text,
and compares action-label bounds to their current buttons. No guessed pointer offsets or color checks.

`ChatForwardReadCancellationTests` cancels one signalled native-style read, starts a replacement
before releasing the old result and joins each actual outer read job. Both old success and old failure
must leave the replacement busy, empty and error-free until its own completion; no submit is admitted.
`ChatForwardCancelDialogTests` mounts the actual dialog/actions/state with an observable outer loader
job: Cancel must close while the controlled read is still unfinished, and its retained application
callback cannot close a fresh dialog. Release joins that original loader before checking the new state.
This fixture isolates dialog cancellation; full shell/host and JNI forwarding remain separate acceptance.

`ChatForwardPresentationTests` cover exact offered selection, read/pre-commit errors, explicit retry,
terminal Accepted/Queued results and retired callback admission. `ChatForwardCompletionTests` holds
the synchronous backend behind an entered/completion signal, creates the replacement before release,
then joins the actual outer send job. Late success and caller cancellation cannot publish a committed
result or failure into that replacement. These controlled callbacks do not prove JNI delivery or picker UI.

Common forwarding target tests use real SQLite state to cover request/open/closed targets, exact
source selectors with no available destination, blocked/own/unknown sources and local Clear. An
unreadable conversation table must fail instead of becoming an empty picker. Native forwarding tests
compare sealed-device/outbox snapshots around the read and recheck a held choice after blocking;
these fixtures are local-store admission coverage, not network delivery or Android picker proof.

`ChatConfirmOwnershipTests` invokes the actual installed Block/Delete confirm and cancel callbacks:
cancel/reopen, a replacement message, an equal selection under a new form, and busy admission before
recomposition all refuse stale authority. The current callback still acts once and observes the selected
message before dismissal. Existing native operation/failure tests remain required; these UI callback
tests do not substitute for core authorization.

`ChatClearOpeningTests` exercise unknown/busy admission, exact-target replacement on retry,
cancel/reopen retirement, disposal and a different active dialog without elapsed-time assertions.
`ChatMenuTests` waits for the actual verified/enabled Clear control before confirming; its injected
reader supplies presentation data only, while native target/mutation tests cover real admission.
`ChatClearDialogTests` reads the actual clipboard, retries a controlled failed target read and invokes
retained application callbacks after the reader opening is replaced. `ChatClearGeometryTests` checks
EN/RU at normal and doubled font scale: current bounds preserve name/ID/copy/consequence order,
scrolling reveals every complete text layout, and both actions remain reachable. These checks require
execution in Android CI before runtime acceptance. The real-node `NativePhotoExchangeTests` also
checks the generated Clear target's owner/peer, own-ID refusal and unchanged complete photo selectors
before its existing cached-original Clear check; the conversation remains open after local cleanup.

Android Clear target checks preserve name, labelled full raw-copy ID and consequence order, including
wrong/missing target, markup, label, order and chunk counterexamples. Tall-dialog checks require
supported safe centering and reachable text/control bounds at real enlarged text sizes.
Android captures also reject modal scrims that fail to cover the viewport or place their dialog
outside its reachable start; the matrix includes the rename dialog. Enlarged-text modal checks
scroll actual controls into view, hit-test their current centres and require the background scroll
position to stay unchanged.

Prototype screenshots serve the mounted assets through an ephemeral loopback HTTP origin,
including the shared identifier ES module. No external network access is needed.
A visible native or ARIA modal is captured in the current viewport: WebKit's full-document
snapshot can reflow its scrollable content and change its scroll position. Pages without a
visible modal retain full-document captures. Snapshot selection completes through the JavaScript
callback before a fresh snapshot failure deadline starts.
Use the manual `prototype-review` workflow or run `make ui-prototype` locally after changing the
prototype or its helpers; JSON measurements go to `build/ux-research/prototype/`.
D1-D3 are checked at each screenshot, including
negative fixtures for long titles, wrong Cancel keys and duplicate/missing defaults.
Selection dialogs may have no default; other dialogs must declare exactly one.
The profile choice dialog has its own screenshot state. Keyboard checks register the native
close listener before Escape and await close plus restored focus. Each JavaScript/snapshot
transition refreshes the failure deadline and parent watchdog only after prior completion.
Native-dialog calibration temporarily closes an existing modal before injecting fixtures and
restores it afterward. WebKit's BODY and native dialog-root boundary stops are bounded separately;
a native root is permitted once only between the last and first controls, in either direction.
Callback tests drive deadlines directly and reject missing close, late completion and focus traps.
They wait for `document.fonts.ready` and two animation frames, with a
30-second failure deadline, before measuring layout. The desktop composer check changes
text through input events and compares the field's current bounds and focus as Send
appears/disappears; it does not assume a window coordinate or add a layout sleep.

The Android read-receipt persistence test waits for the native-loaded enabled switch, invokes
its semantic toggle action, then observes the completed state before reading native storage.
Each transition uses the shared readiness deadline; recreation verifies persisted On before
restoring Off. This persistence check does not depend on a tap coordinate during layout changes.

Android profile-sharing coverage extends the existing two-peer request/reply test:
the peer sends a profile link, the app opens it, shares through the destination
picker, and checks delivery in the peer's history before testing card/back
navigation. On returning to the chat it waits for the enabled composer, which observes a loaded
history snapshot, then reveals the card through lazy-list semantics and taps its current bounds.
The thread shell alone does not establish readiness. Failure context includes bounded thread,
status and delivered-message text. Separate production-bubble tests reveal and tap incoming
and own cards after newer messages, including the own-message menu wrapper.
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
The ordinary API 31 runner explicitly executes its `legacy` phase before the UI suite. Native
connection completion admits the suite; a failed phase stops immediately with the guest-to-node
address, last native connection result and retained report path, without bulk-suite retries or emulator
restart guesses. Successful-connection readiness gives each native operation the transition's remaining
failure budget; it does not repeatedly discard an unfinished handshake after an arbitrary five seconds.
The separate expected-refusal probe keeps its bounded negative-result budget. The helper
`android-lan-permission-phases.sh modern` brackets separate instrumentation processes with shell
permission grant/revoke, checking granted, denied, regranted and revoked states against an owned LAN
node. The first granted phase activates one retained native identity in an isolated
`noBackupFilesDir` registry. Later phases verify its slot/Peer ID and reopen its exact saved active
settings, without reactivating or replacing them. An atomic public checkpoint requires the preceding
phase, both fixture addresses, the same authenticated node Peer ID and a different
boot/PID/process-start marker. A secondary internal Docker bridge uses documentation-only
`198.51.100.0/24`; the existing node binds loopback plus `198.51.100.2`. Every modern phase must
reach the same pinned node through that destination, even when the LAN connection is refused.
It exercises the platform's non-LAN permission path, without an external Internet service or a
claim of global Internet reachability. Neither probe activates new settings. Each successful
phase exports a bounded `local-network-<phase>-receipt.json` with addresses, owner, node, process
and SDK/target; missing or mismatched receipts fail even after passing JUnit.
`android_lan_receipt.py RECEIPT PHASE LAN_NODE NON_LAN_NODE` validates the exported record.
The final granted
cleanup removes only that fixture and restores its prior inventory through the native registry.
Revocation must happen outside instrumentation because the OS can terminate the app process.
Successful connections use native readiness; refusal is checked from the completed native result,
not elapsed time. Each invocation has its own failure deadline and retained report; a failed JUnit
report stops the sequence even when adb exits zero. Without a requested phase the class is skipped.
`ui-test.sh android-permissions` provisions an API 37 Google APIs 16 KiB x86_64 image and runs
these modern phases. It reuses the built APKs read-only, with separate AVD, node and report data;
the extra system image belongs to `android-permission-tools`, outside the release toolchain.
Current paired-route device execution and foreground OS-dialog/callback ownership coverage
remain acceptance gates. API 31 and fake-ADB tests do not prove API 37 socket enforcement.

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

The native photo-protocol retry fixture keeps its established invitation records scheduled as
accepted setup, so the photo is the only due operation. It finds the expected envelope's persisted
due boundary through the public Store API, checks immediately before/at that boundary and retries
once. No wall-clock offset window, global contact resume or arbitrary attempt count synchronizes
this regression; the real responder records the envelope before completing its reply.

## Host client for Android incoming PNG images

`cargo run --locked -p dyapp-client --example android_photo_fixture -- LOOPBACK_MULTIADDR ABSOLUTE_NEW_DATABASE PEER_ID`
starts an ephemeral host client against an existing test node. Only literal loopback TCP addresses
are accepted before lowering fixture proof costs. Owner/device/sealing secrets stay in memory;
the database is newly created and is not reusable after exit. The tool never runs a node.
After profile and key-package publication it prints a public `ready`/`peer` JSON line. Send one
newline-terminated JSON command at a time: `receive`, `text`, `photo`, or `quit` in `op`.
`receive` performs native outbox/inbox work and reports `open`/`more`; `text` sends a fixed fixture
message. `photo` accepts `bytes` (PNG byte array, at most 1 MiB), `width` and `height`, seals and
submits through the normal MLS attachment API, and returns only the committed ID, acceptance
flag and plaintext SHA-256. Zero claimed dimensions deliberately model unknown incoming geometry.
The signature check does not prove pixel validity: Android's normal incoming-image path must check
and decode the actual file. Commands are bounded; malformed input or native errors end the process.
Use `--listen` instead of the Peer ID to accept one guest on `127.0.0.1:33446` inside the
emulator container. Stdout reports `listening` after binding; the guest sends
`{"op":"init","peer":"<Android owner ID>"}` and receives `initialized`, then the native
publication's separate `ready` response with the host Peer ID. The guest reaches this listener
at `10.0.2.2:33446`; the separate UI container's loopback is not the guest's host.
Each TCP command has a fresh frame deadline; fragments never extend it. Reads run outside
the Tokio driver thread, and Quit acknowledges `closed`. Invalid initialization/input ends the process.
The example enables its guard tests in ordinary workspace test discovery.
The Android build exports the host example alongside the APKs and daemon. The emulator container
supervises it through the existing FIFO lifecycle controller. The UI runner invokes
`NativeIncomingPngPhaseTests` with `incomingPngPhase=prepare|reopenOffline|cleanup`, records the
binary SHA-256 and separate `instrumentation-incoming-png-<phase>.txt` files, and stops/waits for
the host. The general suite excludes these phases; no uncertain submission is retried.
Source-bearing runtime evidence remains pending. `IncomingPngChecks` runs the same assertions
against its caller-owned native session. The phase
pairs a normal Android client with the host, receives PNG images
with known and zero claimed geometry, checks exact original bytes and ImageDecoder pixels, then
damages the newly created sealed cache file and requires online recovery. A separate signed 41×20
claim for the actual 40×20 PNG must fail as corrupted without creating a cache entry. A second
negative case changes the parsed IHDR to 8001×8000 and recalculates CRC with Java's standard CRC32;
Android's bounds-only parser must observe that geometry. JNI must reject the oversized frame before
any allocating pixel decoder runs, even with zero claimed dimensions. No large pixel fixture is allocated. It discovers the new file
inside the fixture's native `originals/` folder rather than guessing its blob-hash name or looking for
plaintext pixels in encrypted files. Until the phase actually runs, this helper establishes no
incoming-PNG runtime acceptance and adds no production FFI API.
`RetainedPngFixture` reuses the native worker registry and its public photo-proof checkpoint for
cold offline reopening. Its preparation closes the admitted session before returning; reopening
requires a distinct boot/PID/start marker and verifies the complete selector, SHA-256 and actual
decoded dimensions. The runner acknowledges host and external-node exit before the cold read,
then restores the node and attempts fixture-only cleanup even when the read fails. An uncertain
stop acknowledgement fails without issuing another node command. PNG node acknowledgements use
`png-node-stop.txt`/`png-node-start.txt`, preserving evidence separately from later worker phases.
Cache-file baselines are captured before host submission so an early native consumer cannot erase
the observation window. Actual JNI/offline execution of this retained path remains pending CI.

## Linux sticker exchange

The loopback two-client desktop scenario now includes real File/TAR import, selection in the
saved-pack picker, typed received history and emoji/name fallback after individual-media expiry.
The image check accepts either terminal absence or a retryable transport-failure message, while
requiring the safe metadata and rejecting pixels in both cases.
The runner advances attachment deadlines and quarantines only the isolated sender's generated
blob files to model completed expiry, without waiting for node maintenance or changing signed
messages. The other client clicks Add pack while the sender process is stopped, verifies durable
waiting, then saves the answer after the sender relaunches and sends from that pack. Both clients
stay running during this exchange: the sender completes only after receiving the saved-pack reply,
so a deferred response can retry through completed native mailbox passes. The sender uses a separate
D-Bus session to avoid single-instance forwarding. Another restart verifies the reply and hidden
control-packet projection. Request/image controllers are
awaited through DOM changes and deadlines; native node passes complete before the next check.
The post-restart recovery deadline covers the selection circuit's 60-second cooldown.
No fixed sleep or pixel coordinates are added. These native steps execute in Linux CI; prepared
WebKit runs with mocked IPC establish controller behavior only. Release size comes from CI
artifacts, not a local native build.

## What is tested

The UI suites exercise the native clients (`clients/android/`, `clients/ios/`, `clients/macos/`,
`clients/linux/`, `clients/windows/`). Named test results establish their scoped acceptance, not
complete feature parity (see [ci-matrix.md](../development/ci-matrix.md#acceptance-limits)). Shared
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
Android profile tests also check exact retained input after native rejection: invalid/nonnumeric
height, overlong or bidi-control About text, and reversed/nonnumeric seeking ages. Publishing stays
disabled for these unsaved edits; assertions follow the completed-save signal rather than elapsed time.
The Android publish test checks that closing the dialog clears both confirmations and an unsaved
edit disables publishing until saved, then confirms both, runs the proof without an active network
and expects the localized not-connected failure, retry label and cleared confirmations.
A second Android publish test checks and activates the emulator runner's loopback node
(`10.0.2.2:33445`), publishes a saved draft and expects the signed readback on the form, then deletes the
`network` and `network-draft` files so the other tests still start without an active network.

Native search adapter tests retain offered-field/error fixtures and cover raw BOM/NBSP blanks,
hidden malformed input, numeric overflow and non-desktop whitespace. The iOS adapter's exact height
conditions avoid date-dependent age expectations; native iOS CI must execute the Swift cases.

Android draft-submission tests also pass an absent edit target to the production send path with
an observed coroutine queue. It must dispatch no operation and preserve the edited text, held draft,
target and status. The unopened presentation owner never accesses native storage; failure cleanup
cancels and drains queued work instead of relying on timing.

Android photo-confirmation regression tests invoke the production dialog's semantic callbacks
before recomposition: Send/Send/Cancel must retain one admitted draft, and Cancel/Send must leave
no draft or busy operation. Sender start/release signals control refusal and queued retry;
each transition has its own failure deadline. The callbacks check current state even before
disabled buttons have recomposed. These fixtures do not exercise the OS picker, photo sanitization
or real node delivery; authored/compiled tests still require device execution.

Host Rust tests also call the public native photo history/original exports over trusted stored
metadata and a sealed cache. They check mixed ordering, reopening offline, full message selectors,
block/clear refusal, malformed image framing or descriptor geometry, and a torn cache without
changing MLS/outbox state. These Store fixtures do not prove upload, MLS receive or JNI execution.
Android `AppLockLifecycleTests` exercises the core timer through the production native facade.
Its injected monotonic clock preserves quick-return, exact grace/idle expiration, late-input,
enrollment and clear assertions without sleeps; every fixture closes its native handle.
Android `NativeOriginalDecodeTests` passes directly encoded PNG and JPEG originals through the
production `ImageDecoder` and JNI header plan, checks actual dimensions and rejects truncated
format signatures with the existing safe error. It does not run outgoing preparation, which
would convert PNG into JPEG, and does not prove peer transport or sealed-cache admission.
Android `NativePhotoExchangeTests` uses two native sessions on the suite's external node to
submit and receive a JPEG, select its full message identity and decode identical original bytes
through the production Android decoder. It checks wrong selectors and cached-original refusal
after Block/Clear. Execution requires instrumentation; this does not cover forged incoming PNG
metadata or offline reopening.

The retained worker fixture prepares a separate incoming JPEG through the shared core,
fetches it through JNI and decodes it with the production Android decoder. Its pair-bound
atomic record keeps the full public selector, original SHA-256 and dimensions.
Existing `reopenQueued` phases verify those exact bytes and dimensions while the host's
external node is stopped, including after guest reboot and APK upgrade. The existing
queued-text, identity/inventory and scheduled-worker checks remain. After both peers initialize and
publish, the record also captures their current public device keys through the offline native device
directory. Every reopen compares both keys; absent, replaced or swapped proof fails without creating
credentials or contacting a node. The external node key has a 22-bit proof, while node admission and
preparation accept 8-bit test traffic; fresh receive phases preserve process defaults. This is not a
claim that explicit-seed client mailbox dialing enforces discovery proof minima. Checkpoint tests
reject replaced device keys, swapped owners and malformed or overflowing dimensions. These assertions require actual instrumentation; they add no Android node
or separate emulator lane.

Core lookup tests additionally distinguish equal author/ID values across two groups. Shared
original tests cover JPEG/PNG, unspecified and mismatched dimensions, literal sealed-byte/pixel
boundaries, cache validation, block/clear during a selected fetch, owner isolation and deterministic
cache write failure. A valid 1×1 Adam7 PNG fixture checks admission of both interlace methods
and refusal of reserved methods with corrected header CRCs. Public native tests reopen both
JPEG and PNG with unspecified geometry offline.
These tests do not exercise actual platform image decoders. Shared prepared-photo tests cover
outgoing descriptor/encryption roundtrips, JPEG geometry, optional preview and separate chat/profile
budgets. Public native photo-submission tests require own/blocked/non-open/unpublished, malformed
input and no-network refusals to preserve exact history, sealed MLS state and outbox bytes through
both strict and committed-result exports; these are pre-upload boundaries, not live node acceptance.
A separate loopback libp2p responder exercises the public photo submission over actual signed
attach/blob/mailbox requests: upload FULL/TooLarge preserves state, post-upload mailbox refusal
retains one ID and exact retry bytes after reopening, and accepted submission fetches the exact
uploaded original before reopening its sealed cache offline. Listener readiness and completed wire
responses drive transitions; unrelated swarm traffic does not renew failure deadlines. Queue retry
uses a fresh responder after disk reopening, and stop signals plus thread joins own teardown.
This controlled responder does not prove production node quotas/auth policy, peer MLS receive or JNI.
Sticker metadata projection tests compare records with and without an embedded encrypted transport
descriptor and require identical safe metadata. They do not fetch or decode the opaque source and
do not establish tag-14 wire handling, Android mixed-history rendering or native emoji layout.
The embedded
desktop photo-identity regression holds one original response while completing the other group,
then releases it and redraws both from a warm cache; it checks full IPC selectors and routing
through completion promises without sleeps or pixel-color locators.

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

Android `AppPinInputTests` exercise the actual secure field through the common native PIN-input
policy: ASCII filtering, the independent 64-digit cap, five/six-digit readiness, retained
confirmation-error priority and raw invalid-authentication refusal. Input replacement and current
Compose semantics drive each step; the existing native lock tests still cover verifier records,
attempt limits and actual unlock/settings transitions.

The Android two-peer chat tests (`TwoPeerChatTests.kt`) publish the app's identity on that node and
drive a second identity, `TestPeer`, through the FFI inside the test process: its own folder in the
app cache, its own Keystore alias, the same node and a published profile. The peer sends a text;
the app opens it from its Chats request row and replies, and the peer receives the reply. In a
second test the peer likes the app's identity; the app opens it from Liked you, likes back to
match, messages, edits and deletes the text, and the peer receives each change; the app then
blocks the peer and Chats shows the block note. Opening a matched chat or returning from profile
sharing first awaits the history's explicit state semantics, then asserts the expected Open state
and waits separately for composing to be enabled. Navigation and disabled controls do not prove
snapshot completion. On a shared-card failure the test retains a bounded own-process JVM stack
summary; it does not request another app-native admission while diagnosing a blocked load.
Android publishes local chat history before sending network read receipts. Refreshes coalesce behind
an active local read instead of cancelling it; changing the conversation cancels its presentation
owner, so a late native result cannot update the new conversation. `ChatReadEffectsTests` control
read and acknowledgement completion independently: refresh during a read, delayed receipt delivery,
late completion after owner replacement, and a failed read followed by a successful refresh.
`TestPeer.awaitMessage` checks applied history and advances through completed synchronous FFI
mailbox receives, without an artificial polling delay. Receive/history errors fail immediately.
Each message/edit/delete transition starts a fresh monotonic failure deadline; a late operation
cannot satisfy it. The deadline is checked between completed FFI stages, not a cancellation of
an in-flight native call; network requests retain their own core timeouts. `PeerTransitionTests`
uses an injected clock to cover deadline reset, late completion and error propagation without
waiting for real time. Android instrumentation CI must also run the real two-peer journeys.
`NativeLinkReceiverTests` exercise real JNI and Android credential callbacks for source-bound
invitation creation, non-blocking idle ownership, cancellation before installation, missing active
network and stale-source rejection. The owner-side JNI case checks malformed/expired/valid code
projection and rejects profile A→B→A with the exact view-change error before a private store appears.
Successful wire reception and callback interruption are also covered by the shared Rust
mailbox/registry fixtures. The FFI package additionally drives native poll/install/result through
actual signed mailbox fetch/ack requests carrying authenticated link ciphertext. It checks retained
success after selection changes and typed destination failure after platform wrapping refusal,
then stops the responder before terminal retries. This package-local coverage must reject replacing
`MobileLinkReceiver::result` with `Ok(None)`; Android-only acceptance does not cover that mutant.
`NativeLinkJourneyTests` uses two isolated native registries and the existing CI node: owner
publication, explicit approval, authenticated reception, and normal pending-link enrollment through
`MobileLinkRecovery`. Owner approval now enters the receiving code through production Account →
Devices dialogs and their real native worker. Before creating that code, the test awaits the owner's
verified directory and available entry so initial UI/network setup does not consume invitation
lifetime. It checks original identity and normalized code display/copy, full-control/physical-possession
warnings and actual sending completion before retaining every receiver/import/enrollment assertion.
Owner approval and later revocation share one Compose root with fresh route/scene references.
Each phase awaits actual native handle release before fixture cleanup; no extra node or link setup,
automatic transfer retry or weakened receiver assertion is added. It first opens the normal linked chat session,
which preserves the received device and bootstraps only local session state. It verifies an empty
contact note and favorite list before import, completion followed by an idle retry, then the original
nonempty note and saved favorite flag. `verifyNativeOwnPrivateSync` then observes an actual receive
change in this isolated no-contact-traffic fixture (the own-group Welcome), before staging later
favorite/note updates in opposite directions. Each exchange awaits exact native values through normal
receive completions with a fresh failure deadline; no relink, extra snapshot or guessed receive count
establishes readiness. Source/lint checks do not establish this actual JNI runtime acceptance.
It then reads both native device directories, identifies each current device by the core marker,
revokes the linked device through the owner, and requires both readbacks to retain only the original
public device key. Neither phone hosts a node. The test uses completed native steps and fresh
transition deadlines; its real JNI runtime result and the planned native screen journey remain
separate acceptance. Directory reads do not perform cleanup. The journey additionally requires
readable-mailbox probes to return no cleanup capability, switches to the recipient's other profile,
then completes verified cleanup through the same Kotlin adapter used by both receive paths. A
second adapter call has no authority to recreate the removed context. Only the linked slot disappears;
the other selected identity still opens. Native package tests separately retain duplicate/concurrent
capability outcomes. This cleanup extension needs its own actual CI runtime proof.
`LinkApprovalWorkerTests` and `LinkApprovalLifetimeTests` cover the owner-approval IO adapter with
controlled native-shaped calls. They check core-provided malformed/expired refusal, immutable normalized
public targets, exact opening admission, explicit preparation retry, uncertainty retained after later
expiry, and retirement during both preparation and send. Cancelling a UI waiter cannot cancel the
admitted send or close its handle early; a successor session remains unchanged and each handle closes
once. Completion signals and per-transition failure deadlines replace timing guesses. These tests
require Android instrumentation CI; they do not prove generated binding compilation, actual transfer
or the production owner-approval widgets. The input dialog reuses the localized `owner_link`
resource for its title; Kotlin source lint cannot validate generated `R.string` names.
`LinkApprovalUITests` and `LinkApprovalUILifetimeTests` drive those production dialogs using controlled
native-shaped completions. They assert the original public target and full-control warnings, explicit
consent, partial failure followed by expiry, first-input expiry, acknowledged send and no repeat.
A blocked send disables Cancel/submit and ignores native Back. Replacing the actual opened owner
retires the old dialog; its saved open/submit/dismiss callbacks cannot change an equal successor.
These tests use actual semantics, scroll geometry and completion signals. Existence assertions stay
receiver calls on `SemanticsNodeInteraction`; importing them as top-level extensions fails test-APK
compilation even when main-source CodeQL and source lint pass. Full native EN/RU visual
review and the existing real two-device UI/JNI journey remain separate acceptance.
The owner visual fixtures provide twelve controlled native-shaped outcomes through production
dialog controls, one reusable Compose root, matching EN/RU resources and font configuration, and
completion-owned worker retirement. They never overwrite presentation StateFlows or host a node.
Before real Send/Retry clicks, the visual driver waits for the current Cancel focus restored by the
production completion effect. The worker state alone does not establish native focus readiness.
The ordinary large-text regression physically clicks Send after preparation and requires expiry,
zero sends, the refusal UI and an actual text-scale witness.
This foundation does not establish native authorization, transfer, rendered visual acceptance or
real IME/editor scrolling.
The phase-only `OwnerApprovalVisualTests` requires instrumentation arguments `ownerVisual=1`,
`sourceSha=<40-hex producer commit>` and `expectedWidth=360|412`; ordinary instrumentation excludes
it. A fresh device is required for each width. EN/RU, light/dark and 100/200% font variants produce
top/bottom dialog PNG images and a completion manifest under `files/owner-approval-visual/<width>/`.
Compose PixelCopy captures the dialog. Every captured text layout must report the requested
fontScale; each frame records `actual_font_scale` and `actual_density` from TextLayoutResult.
Owner, receiving and IME archive validation rejects missing, nonfinite or mismatched witnesses.
`LocalizedDialogScaleTests` checks refusal of a bare composition-only override and verifies genuine
1→2→1 dialog scaling while remembered edits and the Activity picker registry survive. The test
context uses a configured parent View without changing global OS settings. Earlier PNG inventories
without effective-scale witnesses do not establish enlarged-dialog acceptance.
Actual paragraph layouts and live scroll viewports establish text geometry; public identifier
semantics retain raw copy values while exposing grouped glyph layout.
Each preparation/operation transition moves safe focus back to Cancel before subsequent input.
Completed validation status/failure also restores focus: a fast worker may finish before Compose
renders its busy state. `LinkApprovalFocusTests` forces that ordering by completing the IO-only
worker within one UI turn, then checks Cancel focus and absence of preparation/transfer.
Visual state waits use `awaitSnapshot` to drive Compose effects while observing session completion;
blocked fixture operations use their explicit entered signal, with a fresh failure deadline per step.
The keyboard-disabled baseline does not establish real IME or internal editor scrolling, native
authorization, transfer, or completed visual acceptance. The manual `android-visual` workflow
consumes the exact producer APK artifact; source-bound native image review still requires an actual
completed run. Source lint cannot establish runtime success.
`tests/scripts/android_visual_producer.py` validates saved GitHub producer/job/artifact API metadata
before the visual runner installs anything. It requires a same-repository main push/manual
`ci.yml` run, exactly one successful matching Android build and an unexpired source-matching runtime
artifact with its digest. Unrelated queued Apple jobs do not prevent this validation; the whole
workflow need not have finished. Keep the API receipts beside `validated.json` for provenance.
`android-visual-collect.sh` first completes `tar` through `adb shell` with its remote exit
status, keeping shell diagnostics separate, then transfers the completed file through `exec-out cat`.
A remote SHA-256 record must match the exact transferred bytes: `exec-out` itself does not carry the
remote command's exit status, and a tar reader may ignore trailing diagnostics after a valid archive.
A missing/malformed digest or mismatch fails collection while retaining the raw evidence.
An unentered phase retains its collection diagnostic without publishing a fake archive. A real
transfer failure keeps partial bytes and a nonzero status; collection failure also fails an otherwise
successful capture, while an earlier instrumentation failure keeps its original status.
`tests/scripts/android_visual_archive.py` reads the collected native tar into a fresh directory,
allowing only regular PNG/manifest files for the requested width with bounded sizes and no duplicate
paths or links. It retains partial safe frames before rejecting an incomplete manifest. Successful
validation requires all twelve states × two languages × two themes × two font sizes × two scroll
edges (192 frames per width), matching source/viewport and PNG header dimensions. This checks evidence
integrity and coverage, not the visual quality of the rendered screens or the real IME behavior.
Run `gh workflow run android-visual.yml -f producer_run=RUN_ID` after that main CI run's Android
build succeeds. The default `suite=standard` retains owner, receiving and IME coverage; pass
`-f suite=comparison` for the separate complete-form matrix, using a producer that contains
`LinkReviewVisualTests`. Use `-f suite=views` or `-f suite=updates` for their independent matrices.
The suites keep independent native archives and validations. Each width gets a fresh KVM emulator without host nodes or network peers. The workflow
checks out the exact producer for toolchain inputs and records the current trusted runner separately;
it never rebuilds APKs. Live density determines the physical viewport, and instrumentation requires
the requested dp width. The keyboard-disabled rendering phase has no submission retries. Always-run
collection retains logs, raw tar and safe partial frames; successful validation requires complete
manifests. Artifacts are `android-visual-producer` and `android-visual-360|412`. Ordinary CI does not
run this matrix; real two-device transfer and real-IME review remain separate blockers.
The phase-only `OwnerApprovalImeTests` fixture requires `ownerIme=1`, `sourceSha` and
`expectedWidth` with the software keyboard enabled. It drives EN/RU, both themes and 100/200% fonts,
selects the beginning/middle/end of the unchanged full code, and waits for native IME visibility.
Android accessibility character-location rectangles and the real dialog window origin/visible frame
establish glyph and control positions; no inferred TextField padding is used. Each width records
24 dialog frames plus measured screen coordinates under `files/owner-approval-ime/<width>/`.
Its preparation fixture rejects any changed input. The manual workflow enables and selects the
native LatinIME for this separate phase, retains its own instrumentation log and tar, and validates
24 complete frames with finite, nonempty glyph/control rectangles inside the recorded visible frame.
Authoring these checks does not establish that the runtime passed.
Owner approval feedback uses the first actual laid-out message line with window/layout readiness
and the platform bring-into-view callback; safe footer focus alone cannot reveal a separate
scrolling body. A new opening starts a fresh body scroll state. `LinkApprovalOutcomeTests` holds
an admitted send, then releases success or uncertain failure without test-driven scrolling;
EN/RU at both actual scales must expose the current feedback. An IO-only repeat completes before
Compose can observe SEND, so equal rendered strings must not hide a new native delivery result.
The tests preserve the original target and
refuse expired/unavailable retry. A large-text negative control scrolls back to the identifiers
and requires the same current-geometry detector to reject the hidden outcome; user scrolling
must not trigger a feedback-reveal loop. Native runtime and fresh visual review remain required.

Dedicated visual instrumentation opts into `--phase-callbacks`. A `DYAppVisual` status callback
renews the failure deadline only for the active JUnit class/method and the next completed case.
Callbacks are emitted after scene/worker cleanup; duplicates, gaps, unrelated logs and late callbacks
do not renew the deadline. They do not count as JUnit completion or prove a passing test. Normal
instrumentation ignores these callbacks unless explicitly enabled.
`DeviceDirectoryWorkerTests` and `DeviceDirectoryFailureTests` exercise the presentation adapter with
controlled native-shaped completions. They retain original public targets across mutable DTO input,
reject stale/forged confirmation openings, retire a view during an admitted call, cancel its UI waiter
and require exactly-once close after completion. A successor view remains unchanged. Partial write
failure, acknowledged success and later cached-read failure remain distinct, without automatic retry.
The same real link journey now confirms revocation through the production Account Devices menu,
retained native screen session and scrollable target dialog instead of calling revoke directly. It
checks the saved menu count independently of the verified remote list, current-row refusal, exact
raw-key display/copy and a matching completion signal, then retains all prior remote-directory,
mailbox cleanup and unrelated-profile checks. Row lookup uses the immutable raw key and current
semantics/scroll geometry. Clipboard verification also awaits the dialog's WindowInfo-driven Cancel
focus before reading the foreground clipboard. Each StateFlow phase has its own failure deadline;
no extra link setup or remote-submission retry is added. This widget extension requires fresh actual CI runtime evidence.
`NativeReceiveUiJourneyTests` complements that unchanged owner-screen journey with production
receiving controls and their actual native store/factory/polling. It starts the invitation through UI,
checks the core-provided code/QR, receives a real counterpart transfer, and finishes enrollment through
its button. The counterpart uses the real native sender directly; owner widgets remain covered by the
original journey. Private note/favorite import, pending completion and the prior recipient profile
are checked separately. No direct receive/install or enrollment-resume call substitutes for the UI
operation; a final readback verifies that no pending completion remains. This new path requires its own
first-pass exact-source CI runtime acceptance.
`DeviceDirectoryUITests` additionally drives the actual Compose controls around controlled native-shaped
calls and the real common collection pager. It checks the immutable displayed target, no current-row
Disconnect, partial-failure Retry, acknowledged success alongside cache failure and an old dialog
callback after an equal successor screen replaces its owner. Native writes are held by explicit
completion gates; observer deadlines reset per transition, and row clicks use current scroll geometry.
An additional controlled case keeps the row count at zero while changing only the core-projected
source from Saved to Remote: captions must distinguish saved fallback from a truly empty verified
directory, without inventing revocation completion. These controlled cases do not prove real network
authorization or full native visual acceptance.
`NativePrivateContactTests` separately invokes the actual JNI favorite/note APIs without a profile
fetch or counterpart mailbox. It reopens native storage, checks identity isolation and exact Unicode
text, and requires private-only mutations to leave public draft, likes and chat state unchanged.
Typed invalid/own/text/blocked refusals preserve existing values; blocked favorites remain removable
and private notes remain readable/editable/clearable. These assertions do not replace the planned
Android favorite-list and note-editor screen journeys or their own-device incremental-sync evidence.
The core protocol case `common_maintenance_starts_one_group_and_syncs_private_changes` uses the real
node and mailbox-ordered own-device pair. It drives the shared maintenance entry point, checks that
only the eligible device starts a group and repeated maintenance keeps it, then exchanges private
changes in both directions. Favorites and note readbacks extend the existing block/mute/name/pass
checks; no sleep or wall-clock turn delay admits a transition. Common owner-admission model cases
also preserve a higher-version unpublished draft, equal/newer cached authority and ordinary clocks
across reopen, reject wrong owner and obsolete context generations, and keep authorized account
tombstones separate from search-only withdrawals. Revocation admission is not a key-cleanup command.
`IdentityActionsTests` additionally gates an admitted switch and delivers a late cleanup completion;
the old source cannot replace the new screen. Batch cases retain a matching completion even when a
different slot completed later, preserve its durability diagnostic, and acknowledge only the observed
batch without dropping newer events. These presentation cases do not prove actual JNI cleanup.
The native directory protocol regression gates the existing real signed-profile responder at its
request callback. The chat lock must remain admitted until response completion, then become available
while the idle handle still exists. A newer empty remote directory leaves the editable draft, package
stock, publication count and platform credentials unchanged; every transition uses completion signals.
`ChatStickerHistoryTests` exercise the existing mixed-history fallback through `MediaChatContent`
with adjacent text and both own/peer stickers, in English and Russian, under the production
`DYAppTheme` typography and root insets. Literal long pack names are located through semantic
list scrolling and checked against measured line bounds, complete text offsets and ellipsis state.
The vertical overflow guard remains; paragraph-container width alone does not establish clipping.
`TextLayoutAssertionsTests` calibrates the guard with actual wrapping, horizontal/vertical clipping,
max-line truncation and ellipsis. Calibration constraints come from the current natural text size.
A full-size Box gives the constrained child loose minimum dimensions; the root window must not
force the Text back to full width/height. Preconditions retain compact actual layout diagnostics,
without guessed offsets, pixel tolerances or media downloads.
This covers fallback presentation, while native sticker exchange and planned picker/image UI
retain their separate acceptance.
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
none). The main suite and explicit peer/photo phase invocations use
`android_instrumentation.py` with raw status callbacks (`am instrument -w -r`). Startup, each
completed named test and final ADB exit have fresh failure budgets (normally 300 seconds, 900 for
the main suite and long worker phases). The monitor has no aggregate suite timer; the CI lane still
has its separate outer 30-minute command budget. Only matched start/completion records advance progress,
including failed or skipped test outcomes; logs, repeated statuses and partial lines do not.
Each monitored invocation flushes validated start/completion events with exact test identity,
completed count, outcome code and monotonic elapsed seconds into its matching `.progress.jsonl`
sidecar. Appending `runner_started`/`runner_finished` markers separates invocations and retains
completed JUnit retry attempts. An outer deadline can stop a healthy long suite too: compare its
last exact completion/start timings before diagnosing a stalled test. Timing events never establish
a passing suite or relax the existing failure deadlines.
The final JUnit checker rejects raw ignored/assumption statuses (-3/-4), even with an `OK`
summary, and writes a success JSON only after all gates pass. The ordinary suite excludes the
three phase-only permission/retained/worker classes and only the forged-search method using
`notClass`; their separate argument-bearing invocations remain required. This avoids intentional
assumption skips without removing ordinary Search tests. A stalled invocation exits 124;
an ADB exit without a final runner result exits 125. These and other host-command errors stop
the suite without a possibly overlapping retry; the original report is retained.
The desktop smoke runs one app
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
Before each ordinary Linux case, the CI-owned provider (`DYAPP_TEST_SECRET_SERVICE=1`) must report
an explicit unlocked collection. Failed or malformed probes stop the case; only a known locked
provider is replaced, and replacement must succeed and return an unlocked readback. No sleeps or
UI recovery bypasses establish readiness, and an unowned provider is never probed or replaced.

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
bash tests/scripts/ui-test.sh android-permissions # API 37 LAN grant/revoke phases
bash tests/scripts/ui-test.sh linux             # = make ui-test-linux
bash tests/scripts/ui-test.sh prototype         # = make ui-prototype; desktop prototype screenshots
bash tests/scripts/ui-test.sh prototype-android # = make ui-prototype-android; Android prototype screenshots
bash tests/scripts/ui-test.sh all               # Android unit, API 31, API 37, Linux
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
  `android-emulator`, `android-permissions` and `all`): copies the sources to `/tmp` with `build/` linked to
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
    Before the emulator, it generates a 22-bit production-proof key and starts that `dyappd` with the
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
- **`android-permissions`** → service `android-permission-test`, depending on a healthy
  `android-permission-emulator`: installs the same base debug and instrumentation APKs, then runs
  the explicit modern LAN phases. The emulator uses
  `system-images;android-37.0;google_apis_ps16k;x86_64`; its toolchain stage adds only that image.
  KVM and boot/package-manager readiness follow the API 31 lane, but its node key keeps the
  default 8-bit proof because no fresh-process worker runs here. Its AVD is
  `build/android-permission-avd`; node data, phase reports and logcat are in
  `build/android-permission-reports`, uploaded in `android-permission-check-output`.
  Locally `docker-local.py` stops dependencies after each suite, so the two emulator lanes run
  sequentially. CI publishes the APKs and `dyappd` from one 45-minute build/unit job, then runs the
  two lanes in parallel on separate 45-minute runners with `fail-fast: false`. Their test budgets
  are 30 minutes for API 31 and 15 minutes for API 37. Both retain host processes, container
  state/processes and one-off Compose logs on a stall; the main lane keeps the
  `android-check-output` artifact name.

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
  After font readiness, the native smoke measures the comparison instruction against its current
  visible predecessor and the shared spacing token, both before and after a scan status appears.
  Hidden status/preview siblings must not remove that gap.
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

Desktop cache UI fixtures script native verdicts rather than copying cache admission policy.
They await admission before inspecting confirmations and preserve the existing cancellation,
identity and uncertain-write checks. The actual native settings smoke also refuses a decimal
that floating-point conversion would round to a whole-byte budget, checking the unchanged stored
limit and exact raw input.

Desktop profile-scale checks distinguish a real select with no selected option from an explicitly
selected zero-valued unset. All offered scales exercise serialization refusal; actual editor saves
also assert zero persistence calls for missing required/optional selections and preserve unrelated
fields on explicit unset. Native Store checks refuse unknown required/optional codes without
replacing the saved draft.

Desktop profile-input checks await the actual core interpretation and the field-assessment
completion promise. Held assessment failures are delivered after replacing the text and after
reopening an equal editor through real identity navigation; neither may invalidate the current
form. Publish/solve fixtures acknowledge actual invocation through their held-call signal, with
a fresh check deadline for each awaited transition.

### Desktop prototype screenshots

Run `.github/workflows/prototype-review.yml` manually from GitHub Actions for current desktop
prototype review evidence without building WebKit tooling locally. The `docker - prototypes`
job builds the existing capture image, then requires a real `stickers-ready` capture (light,
100%, English including `en-XA`) to complete within a five-minute failure deadline before starting
the full desktop matrix and canonical layout/criteria checks. The preflight uses the same capture
script with a separate `build/prototype-preflight` output mount; a startup timeout fails the job,
never admits the full matrix. The workflow retains `prototype-review` for seven days, including
preflight images and failed-capture diagnostics.
The image includes Git for the criteria baseline ratchet; checkout retains full history.
The artifact includes screenshots, measured/expected criteria, layout, bounded diagnostic logs,
source commit, tracked input hashes and image ID. A successful capture supplies evidence for
qualitative review; it does not complete that review or prove native client behavior.

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
fail. Clipping checks measure the page, or the focused visible native/ARIA modal and its
viewport bounds. Inactive background is checked in its standalone states; it is not counted
as active modal content. Counterexamples cover foreground clipping, hidden and off-viewport
modals, and restored background coverage after closing. This follows the
[modal interaction model](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/).
Known accessibility/reflow findings live in `tests/scripts/desktop-ui-baseline.json`.
New findings and stale entries fail. The baseline is intersected with every committed version,
so it only shrinks, including manual CI runs; full Git history is required. Dialog and keyboard
failures cannot be budgeted. `check-ui-criteria.py` validates each slice against its expected
shots and the complete matrix. Contrast checks isolate paint behind an opaque native or
ARIA modal, preserving layout. Incomplete contrast for an unambiguously identified node is
rechecked after scrolling it into view and paint completion; only an affirmative pass removes
the incomplete result, and a confirmed failure remains a violation. Unresolvable targets stay
incomplete. Follow-up measurements retain the original DOM-node reference; replacing a node
with another under the same selector cannot clear its earlier result. The audit restores page
scroll and focus, and good/poor scrolled-text fixtures check both contrast decisions and restoration.

Visible out-of-flow opaque menus use separate paint contexts for the foreground and covered
background. Original violations remain. An incomplete contrast result changes only after a
positive measurement of the same unique DOM node. Foreground isolation requires an unchanged
in-viewport popup with plain ancestors and no possible higher paint. Covered-background rechecks
require actual text ranges intersecting the menu body and expose that layer as when the menu
is dismissed; shadow-only or otherwise uncovered targets keep their original context. Clipped,
translucent or potentially covered menus are not eligible. Text clipping still measures menu
content and viewport bounds. A temporary fixed position removes a valid popup's contribution to
ancestor scroll bounds only when every descendant rectangle remains identical; all styles,
priorities and scrolling are restored even on errors. Fixed opaque navigation sidebars use the
same checks. A positioned pseudo-element is proven below the menu only with plain ancestors,
no owner stacking context and a lower root layer; raised or unknown contexts remain ineligible.
Six additional real-DOM cases exercise those decisions, including paint with `pointer-events:none`.
Native select scroll metrics can miss clipped selected text, so touched menu values also require
rendered inspection; a zero metric count alone does not establish full-label visibility.

Sixteen independent popup calibration cases cover readable layers, poor foreground/background,
application-bar ordering, higher overlays with both pointer policies, translucency, clipping
ancestors, viewport overflow, real label clipping, measurement/pinning/paint errors, declaration
priority, uncovered targets and same-selector node replacement. Each case runs through its own
keyboard-fixture completion callback and fresh failure deadline. Style restoration compares
declared properties, values and priorities instead of empty-attribute serialization.
The existing L2/I1 measurement stays in the frozen normal-text
English/Russian 440/1280 light matrix and is checked through `layout.json` and `[layout]`.

The frozen matrix also walks real GDK Tab/Shift+Tab events in DOM order and uses arrow keys
for every native radio-group member. Rendered focused/unfocused pixels must meet WCAG 2.4.13:
3:1 contrast over at least a two-CSS-pixel perimeter area. Entirely obscured focus fails
SC 2.4.11. Escape closes dialogs and restores the declared opener; unsolicited revocation
focuses the next screen heading. Counterexamples prove failures for traps, invisible focus
and ignored Escape, in addition to axe, dialog and clipping fixtures. The same calibration path
checks queued Refresh success and failure for all four prototype collections. A controlled thenable
settles immediately before same-turn away-and-back navigation; original row identities must survive.
Each case restores its records, loader, screen and focus and has its own completion deadline. A focus-area failure
also saves the last compared frames as `keyboard-focus-before.png` and
`keyboard-focus-after.png` beside the captures. The prototype models
these keyboard/dialog transitions without backend actions. A dialog declaring `data-initial-focus`
is checked before the keyboard walker sets its first anchor, so traversal cannot hide an incorrect
initial focus target. Device-code variants declare the code field; contact naming declares its
name field. Rename calibration checks opening and reopening, then deliberately blurs that input
and requires the pre-walk audit to report the missing initial focus. The named `switcher` state
also requires its opener and both identity-menu actions to be enabled, visible and hit-testable
before traversal; a hidden sidebar cannot turn that requested popup into a passing profile shot.
Completed keyboard audits release their retained screenshot before invoking the next-page
callback, on success and failure; ownership regression tests require no garbage-collection wait.
D1's two-line limit stays in the
frozen matrix; stress variants check clipping and accessibility instead.

The ordinary push/PR workflow does not run prototype captures; use the manual workflow above
for the full matrix. Locally, set `DYAPP_PROTOTYPE_THEME`, `DYAPP_PROTOTYPE_TEXT_SCALE` and
`DYAPP_PROTOTYPE_LANGUAGE` to check one theme, text scale or language.

### Android prototype screenshots

`ui-test.sh prototype-android` (`make ui-prototype-android`) does the same for
`clients/android/prototype/index.html` with the same image: every state of
[Mobile screens](../design/mobile-screens.md) in English and Russian, light and dark, at 360 and
412 px wide, at normal and 200% text-only WebKit zoom. Enlarged-text names include `--text200`:
`build/ux-research/prototype-android/<state>[-dark][--text200]-<lang>-<width>.png`.
The capture rejects a document whose initialised screen does not match the requested state,
including an HTTP error page. After font readiness, it rejects horizontal page overflow by comparing
actual root scroll and client widths; full-document PNG dimensions alone are not viewport evidence.
The standard runner serves Android at `/assets/android/prototype`;
custom paired probes may use a different prefix, which must match their requested URL.
All Settings categories and the three collection detail screens are in the capture inventory.
Collection probes also exercise more than 32 records, search/page bounds, counts and stale refresh
completion through the shared desktop/Android prototype fixture; static fixtures are not native proof.
The open language picker is captured as `settings-language-dialog`; choices follow the prototype
catalogues rather than a fixed three-radio inventory. Catalogue stress checks register additional
languages before initialisation and verify complete labels, locale values, Cancel/Escape and focus.
The capture also requires shared labelled identifier presentation on every code value and rejects
presentation styles that override a hidden state. Identifier checks retain the raw selection and
copy text through asynchronous replacements; list/action checks use current geometry.
The capture rejects text outside the actual bounds of top/back-bar titles, status chips, navigation
labels, unread badges and the wrapping relationship-goal picker. Birth-year checks reject text wider
than even the content-box upper bound; they do not guess the native spinner width. The picker state
also checks native-dialog viewport bounds. This uses font readiness and rendered text rectangles,
not colour samples or guessed positions. The desktop prototype is mounted too, because the Android
one loads its strings. The layout check runs with the `android` baseline.

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
| `android-emulator-test` | `instrumentation.txt`/`.json`, `*.progress.jsonl`, `instrumentation-<n>.txt` of failed runs, `ready.png`, `logcat.txt`, on failure `failure.png`, `window.txt`; plus `app-debug.apk` |
| `linux-test` | `desktop-ui-results.json`; plus the `dyapp-linux` binary |

Both Android instrumentation runners write a fresh `android-artifacts.json` beside
their test reports before installing packages. It records base/upgrade/test APK hashes,
all native payload hashes and the actual prepared JNA manifest; mismatched JNA,
missing x86_64 JNA/FFI, incomplete JNA/FFI pairs in any additional ABI, or duplicate
native entries fail before installation. The emulator upgrade build retains the base build's
explicit x86_64 selection when reusing its compiled Rust core.
CI's `build-provenance/` records the source HEAD, tools-image ID and actual
container hostname/image/service tuples. Match the hostname to the report directory
to bind a test result to its APK receipt without timestamps or guessed ordering.

CI publishes report files, APK/native SHA-256 inventories and captured Docker logs
in `android-check-output`, including evidence collected after a test failure.

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
