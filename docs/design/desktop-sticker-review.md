# Desktop sticker interaction review

This checkpoint extends the [desktop heuristic record](desktop-heuristics.md) and applies
the [review criteria](review-criteria.md). Linux v0.0.1 is the scope; Android and future
video work are separate.

## Clear chat and pending requests (2026-11-07)

Reviewed before changing the confirmation: Clear dialog, current/pending thread, empty
thread and saved collection, in English and Russian at 440 and 1280 px. The maintainer
chose cancellation of pending pack requests on full Clear chat. Saved packs, recents and
historical sharing stay; accepted mailbox packets cannot be recalled.

| Finding | Severity | Evidence and required fix | State |
|---|---|---|---|
| C1 | 2 | Native/prototype warnings now explain pending pack cancellation and saved-pack retention in both languages. | Closed |
| C2 | 3 | Plain-text contact name and labelled full Peer ID precede the consequence. A semantic definition list labels the target, not our own identity. Raw copy and eight-grapheme breaks are preserved. | Closed |

| Heuristic | Review requirement |
|---|---|
| H1 status | Keep the visible busy/error/cleared states; queued requests do not become new chat notifications. |
| H2 familiar language | Say pending pack requests are cancelled; avoid transport jargon in the confirmation. |
| H3 control | Preserve Cancel, Escape before submission and focus return; do not claim queued packets can be recalled. |
| H4 consistency | Retain the existing destructive dialog and shared identifier presentation. |
| H5 prevention | Name the target and consequence before confirmation; preserve saved packs explicitly. |
| H6 recognition | Show contact name and raw-copyable full Peer ID rather than relying on background context. |
| H7 efficiency | Keep the existing keyboard path and one destructive action. |
| H8 restraint | Add concise copy and target identity; no new settings or transport controls. |
| H9 recovery | Existing failure leaves the dialog open with retry; cancellation and history clear roll back together. |
| H10 guidance | Manuals explain local scope and pending-request cancellation. |

L1/L2 and I1-I4 apply to the target block and actions. D1-D3 and T1-T2 apply to the
localized title, warning, Cancel and confirmation. No Settings preference changes are made.
Actual-controller WebKit checks cover target order, literal markup, raw copy/chunking, neutral
Peer ID label and Cancel focus/return. Twenty-four native-controller captures cover confirmation,
error and busy, short/long names, EN/RU at 440/1280: `build/ux-research/linux-clear-packs/`.
Measured identifier gaps are at least 12 px above and below; action gap is at least 16 px.
The title stays one line and no confirmation requires scrolling. Prototype captures of Clear,
current chat and cleared chat add 12 states with no L2/I1 findings:
`build/ux-research/linux-clear-prototype/`. Saved collection review uses unchanged earlier evidence.
Shared contrast/focus tokens are unchanged. Native application execution and real IPC remain CI
checks; mocked IPC does not prove them. C1/C2 are closed; no finding is accepted as an exception.

## Picker and received packs: design checkpoint (2026-11-07)

Before prototype changes, reviewed the current composer, collection manager, plain sticker
fallback and message actions. The composer currently opens management rather than sending;
the native commands already provide bounded pixels and guarded request actions. Keep the
manager as a separate destination, with an explicit return to the picker. The proposed picker
uses a dialog with pack choices, eight image buttons, Previous/Next and Cancel. Recents omit
unavailable entries. A standalone sticker preserves an unrelated text draft.

| Heuristic | Interaction and acceptance requirement |
|---|---|
| H1 status | Announce loading, empty, sending, queued, waiting, downloading, added and failure distinctly. A completed durable send closes the picker; delivery stays in the thread. |
| H2 familiar language | Use pack names, emoji and ordinary Request/Retry/Stop waiting wording; no mailbox, token or descriptor terms. |
| H3 control | Cancel/Escape returns focus; closing never claims to recall a committed send. Stop waiting cancels acceptance; Clear chat also cancels pending requests. |
| H4 consistency | Use existing dialogs, button/focus tokens, collection manager and shared media auto-download policy. |
| H5 prevention | Disable duplicate actions while busy; retain identity/view/selection guards; request packets only after an explicit action. |
| H6 recognition | Image buttons have emoji and pack accessibility labels; fallback names the sticker and pack. Pack names wrap rather than losing their distinguishing suffix. |
| H7 efficiency | Keyboard buttons and bounded paging; viewport-driven message loading; explicit image retry and pack request remain separate. |
| H8 restraint | No caption or bubble, image at most 192 pt; no Edit/Forward, own Delete remains available. Safe pixels only. |
| H9 recovery | Keep fallback on unavailable media; distinguish quota/storage/invalid/fetch failures. Retry keeps a useful answer; a fresh request is explicit. |
| H10 guidance | Explain that saved packs stay until removed, pending requests can stop, and offline requests wait for connection. |

The dialog uses normal pack-choice buttons with an announced selected state, avoiding an
incomplete ARIA tab keyboard contract. The bounded grid has eight independent labelled buttons.
L1/L2, D1-D3 and T1-T2 apply to the dialog, grid, request actions and neighbouring composer.
No new Settings preference or identifier confirmation is introduced; collection removal keeps
its existing I1-I4 review. Reusing the collection manager reduces duplicate import/quota UI;
the extra transition costs one action and requires focus restoration on return.

Prototype validation captured 64 states (13 sticker variants plus chat, draft and collection
neighbours; EN/RU at 440/1280) under `build/ux-research/linux-sticker-picker/`. Font/layout
readiness callbacks and failure deadlines replace sleeps. All 64 pass L2/I1 measurements;
geometry assertions check full pack names, bounded labelled canvas choices, paging order,
viewport containment and message image size. Visual review covers narrow Russian and wide
English picker/error and received-failure states; shared contrast/focus tokens are unchanged.

| Finding | Severity | Fix and evidence | State |
|---|---|---|---|
| P1 | 3 | Long Russian pack names initially clipped horizontally. Choices now wrap within a bounded scrolling region; all captures pass the dynamic text-width check. | Closed |
| P2 | 2 | The generic narrow-dialog rule reversed Previous/Next. A specific paging rule retains their reading order; the geometry assertion passes at both widths. | Closed |

No finding is accepted as an exception. The shipped picker now uses the reviewed canvas grid,
with separate bounded pack/image paging. Controller checks run against the actual markup and
scripts: chosen pack/index, draft preservation, duplicate-click guard, retry, manager return,
old reads after reopening and peer changes, malformed pixels, focus and pack/grid spacing.
Sixteen ready/empty/error/sending captures cover EN/RU at 440/1280 under
`build/ux-research/linux-sticker-picker-live/`. An added pack pager initially touched the grid;
shared token spacing fixes it and the dynamic controller geometry check passes. Native IPC
execution remains a CI check.

The shipped message controller now uses viewport callbacks and safe binary RGBA framing.
Sixteen image/fallback/unavailable/malformed-frame captures cover EN/RU at 440/1280 in
`build/ux-research/linux-sticker-messages-live/`. Actual-controller checks cover automatic
policy denial, manual retry, exact owner/message binding, raw-byte representations and offsets,
malformed/trailing bytes, stale replies after Clear, accessible canvas, own Delete and hidden
Edit/Forward. Measured image dimensions stay within 192 pt and sibling gaps meet token minima.
Keyboard focus follows a manually loaded image, including WebKit retaining the hidden button
as active until layout; the recovery also handles focus released to the body without stealing
focus moved elsewhere. The same correction applies to picker paging. Contrast tokens are unchanged.
Request controls and acceptance across repeated mailbox redraws remain separate checkpoints.
Prototype and mocked-controller validation do not establish native transport acceptance.
