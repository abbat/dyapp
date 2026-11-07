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

## Remaining sticker screens

The native core and IPC support safe pixels, bounded thumbnails, queued sends and pack
request status/actions. Picker, recents and Add-pack screen/controller work remains planned.
Review those states and their neighbours before changing their screen code; do not infer
native transport acceptance from mocked IPC or static screenshots.
