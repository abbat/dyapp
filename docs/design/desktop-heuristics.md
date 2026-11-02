# Desktop heuristic review

A review of every state of the desktop prototype (`clients/desktop/prototype/`, see
[desktop screens](desktop-screens.md)) against Jakob Nielsen's
[10 usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/). It was
done on 2026-10-29, before the shell was built, and every finding is fixed in the prototype. The
shipped UI (`clients/desktop/ui/`) was reviewed the same day when it got the shell; its findings
are under [Shipped UI](#shipped-ui) and are fixed too. The rule that keeps it current is under
[When to review](#when-to-review).

## Method

Each state was opened in English and Russian at 440 and 1280 px (`make ui-prototype`) and
checked against the heuristics:

| Code | Heuristic |
| --- | --- |
| H1 | Visibility of system status |
| H2 | Match between the system and the real world |
| H3 | User control and freedom |
| H4 | Consistency and standards |
| H5 | Error prevention |
| H6 | Recognition rather than recall |
| H7 | Flexibility and efficiency of use |
| H8 | Aesthetic and minimalist design |
| H9 | Help users recognize, diagnose and recover from errors |
| H10 | Help and documentation |

Severity follows Nielsen's scale: 0 not a problem, 1 cosmetic, 2 minor, 3 major, 4 catastrophe.
A finding names the state, the heuristic, what a user meets and the fix. H7 (shortcuts) and H10
(help) were checked but are out of scope for the prototype: keyboard shortcuts and a help page
are not designed yet.

## Findings

### Severity 3

| State | Heuristic | Problem | Fix |
| --- | --- | --- | --- |
| `profile-hidden` | H1 | The status still said "Published, version 3. Visible in search." | The status says the profile is hidden and the version stays saved as the draft |
| `profile-view-match` | H3, H5 | One press on Liked took the like back and ended the match for both | Liked opens a confirmation, `unlike-dialog` |
| `settings` | H3 | Block could not be undone: no list of blocked people | A Blocked people card with Unblock (`blocked`, `unblock-1`) |

### Severity 2

| State | Heuristic | Problem | Fix |
| --- | --- | --- | --- |
| `search-error` | H1, H9 | "Unavailable while offline" while the sidebar said Connected; no way on | The sidebar and top bar show Offline in this state; Open Network next to Retry |
| `network-error` | H1, H9 | "Previous network stays active" next to Offline; no advice; Details open against the spec; Copy indented | The sidebar keeps the previous network's status; the text says nothing changed and to check the addresses with the operator; Details collapsed; Copy aligned |
| Wizard, recovery, removal | H4 | Full Peer IDs in the libp2p `12D3KooW…` form, not the 64 hex characters of ADR 0033; the tail wrapped alone at 440 px | 64 lowercase hex characters on their own line |
| Every identity list | H4 | Short IDs with capitals (`@Fk9Qx2`) | Lowercase hex tails, avatar letters to match |
| `remove-dialog` | H5, H6 | Only the full ID named the identity; no word that its chats go too | Avatar, short ID and kind first; a line that chats, matches and likes on this computer go with it |
| `chat-request` | H6 | "Reply … or block" while Block hid in the ⋯ menu | A Block button in the request notice |
| `chat` | H5 | Delete on an own message acted at once | Delete opens `delete-dialog` |
| `profile-view-match` | H5 | Block had the size and row of Liked, easy to hit by mistake at 440 px | Block is a quiet button under the profile fields |

### Severity 1

| State | Heuristic | Problem | Fix |
| --- | --- | --- | --- |
| `network` | H1 | "One node answered" while both rows said Answered | "Both nodes answered" |
| `search-loading` | H3 | No way to stop a slow search | Cancel |
| `search-empty` | H2 | "Show another sample" under "Nobody matches yet" | Removed; the advice points at the filters |
| `chat-first` | H2, H8 | "One message is delivered now" next to the Sent mark; text repeated under the composer | The notice states the rule only |
| Chat states | H5 | Send enabled with an empty box | Send is disabled until there is text |
| Chat states | H8 | The composer floated mid-window in short chats | The thread grows and the composer stays at the bottom |
| Shell at 640 px and below | H1 | Connection only as a coloured dot | The top bar shows the status word too |
| `profile-dark` at 440 px | H8 | Selects too narrow, text cut | Grid columns at least 200 px or full width |
| `chats` | H1 | Nav badge 3 against row counts 2, 1 and 2 | The badge counts unread messages and requests (2 + 1) and its label says so |
| `recovery` | H5 | Remove first in the row, next to "Use another identity" | "Remove this identity…" is a quiet link under the Peer ID, apart from the row |

States with no finding: `wizard-identity`, `picker`, `publish-dialog`, `publish-progress`,
`network-checking`, `search`, `profile-view`, `chat-menu`, `chat-clear-dialog`, `chat-cleared`,
`switcher`, `menu`, `remove-offline`.

### Shipped UI

The states `make ui-test-linux` saves to `build/ux-research/current/`, checked the same way.

| State | Heuristic | Severity | Problem | Fix |
| --- | --- | --- | --- | --- |
| `settings` | H2, H4 | 2 | Remove sat apart from the identity it removes | Remove… is in the active identity's row |
| Picker and key storage from Settings | H3 | 2 | No way back to the open identity | Cancel |
| `profile-publish-failed`, `search-not-connected` | H1, H9 | 2 | A plain grey line when no node answered; no way on | The message is an error and offers Open Network |
| `network-failure` | H9 | 2 | A raw error string | A localized row; the raw text in a collapsed Details |
| `recovery` after a corrupted key file | H2, H9 | 2 | "Its storage cannot be opened now" blamed the storage and gave no next step | The text says the key could not be read and to check the storage and the key file, then retry |
| `recovery` | H8 | 1 | An empty Peer ID row; extra space under the notice | The empty row is hidden; the notice's last margin is 0 |
| Remove | H4 | 1 | The label opens a dialog but had no ellipsis | "Remove this identity…" |
| `network-connected` | H8 | 1 | The node ID in the main text | The node ID is in Details |
| `search-not-connected` | H8 | 1 | Retry repeated the Search button | Retry is hidden; Open Network shows instead |
| Profile states | H8 | 1 | Forms stuck together; inputs and selects of different heights | A gap between grids; inputs and selects are 2.5rem high |
| Profile and search states | H8 | 1 | Select text sat below the middle of the box | The select keeps only its own vertical padding |
| `search-results`, `search-empty` | H8 | 1 | A wrapped label pushed its box below the rest of the row | Grid rows align to the bottom |
| `search-results` | H8 | 1 | A list bullet before each result's avatar | No bullets |
| `search-error` | H8 | 1 | The node-problem bullet hung outside the notice | No bullets |

States with no finding: `picker`, `setup`, `wizard-network`, `recovery-details`, `ready`,
`network-empty`, `profile-edit`, `profile-preview`, `profile-published`, `remove`.

### Safety-number review (2026-10-31)

Reviewed `profile-view-safety` and its `profile-view` and `profile-view-match`
neighbours against H1–H10 in English and Russian at 440 and 1280 px. Twelve
WebKitGTK prototype screenshots are in `build/ux-research/safety-review-20261031/`.
The unverified status, all 60 digits, trusted-channel instruction and return
navigation are visible without clipping; the adjacent states keep their message
and like actions. No open finding. This is a static layout review; interaction
and real-device performance remain separate checks.

### Contact QR layout review (2026-10-31)

Reviewed the changed `profile-view-safety` panel and neighbouring `profile-view`
and `profile-view-match` states in English and Russian at 440 and 1280 px; twelve
WebKitGTK prototype screenshots are in `build/ux-research/safety-qr-review-20261031/`.
The QR layout stand-in keeps the number, status, instruction and navigation
visible without horizontal clipping. The neighbouring actions remain visible.
No open H1–H10 finding. The stand-in is deliberately not scanable; the shipped
canvas receives a real encoded matrix and needs the native UI check.

### QR image import layout review (2026-10-31)

Reviewed `profile-view-safety` and neighbouring `profile-view` and
`profile-view-match` states in English and Russian at 440 and 1280 px. Twelve
WebKitGTK prototype screenshots are in
`build/ux-research/safety-import-review-final-20261031/`. The import button,
number, QR, instruction, navigation and neighbouring actions remain visible
without clipping. Increased the gap between the button and instruction after
the first screenshot review. No open H1–H10 finding. The prototype uses a
layout stand-in; native image import is checked by the UI smoke.

### Contact camera control layout review (2026-10-31)

Reviewed `profile-view-safety` and neighbouring `profile-view` and
`profile-view-match` states in English and Russian at 440 and 1280 px. Twelve
WebKitGTK prototype screenshots are in
`build/ux-research/safety-camera-review-20261031/`. The image and camera
buttons wrap cleanly at narrow widths, while the QR, instruction and adjacent
profile actions remain visible. No open H1–H10 finding. The prototype does not
exercise a physical camera; native CI and real-device checks are separate.

### Safety-number copy layout review (2026-10-31)

Reviewed `profile-view-safety` and neighbouring `profile-view` and
`profile-view-match` states in English and Russian at 440 and 1280 px. Twelve
WebKitGTK prototype screenshots are in
`build/ux-research/safety-copy-review-20261031/`. The copy action stays adjacent
to the grouped number, the QR and scan controls remain readable, and the
neighbouring profile actions remain visible. No open H1–H10 finding. Clipboard
expiry is covered by a separate behavioral test, not by these static images.

### Android lock screen layout review (2026-11-01)

Read `lock-setup` and `lock-unlock` in the Compose code against the prototype and H1–H10 in
English and Russian; not yet seen on a device, native rendering is left to the CI Android job.
The whole page scrolls, so at large text every action stays reachable (H3, H7). Two choices
keep the state visible: without a device screen lock `lock-unavailable` replaces the recovery
caption, which would promise what the device cannot do (H1, H5), and once too many wrong PINs
gate the field `lock-device` becomes the filled primary action, since it is the only way in
(H3, H9). No open H1–H10 finding.

### Android picker and recovery layout review (2026-11-01)

Read `picker` and `recovery` in the Compose code against the prototype and H1–H10 in English and
Russian; not yet seen on a device. Recovery leads with the reason and keeps the reason code and
Peer ID behind Details with a copy action, for a support request (H9, H10). Removal stays the
least prominent action, in the error colour (H5). `lock-settings` is kept as a last text action
beyond the prototype, since no other path reaches it while no identity opens (H3). No open
H1–H10 finding.

### Android network wizard layout review (2026-11-01)

Read `wizard-network` in the Compose code against the prototype and H1–H10 in English and
Russian; not yet seen on a device. One primary action shows at a time, and a finished activation
now offers a way forward where the old screen left only Back (H3). `skip-network` stays available
until activation (H3). The saved-draft line no longer promises a check that already exists (H1).
No open H1–H10 finding.

### Android identity step review (2026-11-01)

Read `wizard-identity` in the Compose code against the prototype and H1–H10 in English and
Russian; not yet seen on a device. The old flow created a key without asking; the step now states
where the key lives and that it has no backup before the one action that creates it (H1, H5). A
failed creation falls to the existing storage error instead of looping (H9). No open H1–H10
finding.

### Android profile and publish dialog review (2026-11-01)

Read `profile` and `publish-dialog` in the Compose code against the prototype and H1–H10 in English
and Russian; not yet seen on a device, native rendering is left to the CI Android job. The form now
groups fields into cards and folds the other scales under `profile-more` (H8). Publish opens a
dialog that lists exactly the saved public fields before asking for the two confirmations (H1, H5),
and an unsaved edit disables it with a stated reason (H5, H9). A failure keeps the dialog open with
its retry label and asks for the confirmations again (H9). While running, only the proof can be
cancelled and the dialog cannot be dismissed, so an attempt never continues unseen (H3). No open
H1–H10 finding.

### Boot error recovery review (2026-11-02)

Read the boot error, loading and Ready states and the privacy error against H1–H10 in English and
Russian; layout at 440 and 1280 px comes from the shared card and `.actions` rules, not a new
render. A failed start, switch, creation or removal used to end on the boot card with no control
and a line saying to restart the app (H3, H9, severity 3): the card now has Retry, focused on
arrival, which rereads the state and restarts the node of the kept identity. Retry first sat loose
in the card (H4, severity 1) and now uses `.actions` like every other card. The error line claimed
the identity was not replaced, which is false after a removal that fails past erasing the key (H2,
H9, severity 2): it now says only that the action did not finish and what to check. No open H1–H10
finding.

### Contact verification review (2026-11-02)

Read the safety panel, `verify-dialog`, the profile view with `view-verified` and every
`view.safety.confirm.*` string against H1–H10 in English and Russian; the dialog and the panel
reuse `.modal`, `.notice` and `.actions`, and the UI smoke audits them at 440 and 1280 px. The
panel said a match does not verify and offered no way to verify (H1, H7, severity 3): `view-confirm`
now does, and `view.safety.compare` and `view.safety.scan.matched` say so. One click writing a
security mark from a long panel was too easy (H5, severity 3): `verify-dialog` states the condition
and focuses Cancel. A mistaken mark could not be undone (H3, severity 3): `view-unverify` removes
it. The panel kept saying Not verified after a confirmation and after a reopen (H1, severity 3):
`view-safety-state` and `view-verified` follow the stored mark, and the notice stays visible with
the panel closed. The network step can take seconds, so the panel names it (H1), every failure says
nothing was marked and what to do (H9), and focus returns to the control that remains, not to
`body`, after the disabled button goes away (H3). Own-profile cards keep the button and get a
specific refusal, because the page does not hold its own Peer ID here (H5, severity 1, fixed by the
message). No open H1–H10 finding.

### New-device notice review (2026-11-02)

Read `chat-new-device`, `chat-new-device-notice` and every `chats.newDevice*` string against H1–H10
in English and Russian; the notice reuses `.notice` and `.btn quiet`, and the UI smoke audits it at
440 and 1280 px. A silently lost mark would leave the user trusting a key they never checked (H1,
severity 4): the chat says what changed and why the mark went, and the notice stays across a restart
and every mail pass. A raw "keys changed" line gives no next step (H2, H9, severity 3): the text
says when to act and where, in the app's own terms. A dismissal that looked like approval would
restore false trust (H5, severity 3): the button says I understand and leaves the mark off. A failed
dismissal keeps the notice and says to try again (H9). The disabled button goes away after success,
so focus moves to the composer, or to Back when the composer is disabled (H3). A rebuild for each
mail pass does not flicker the notice, because it is toggled only by the flag (H4). No open H1–H10
finding.

### Public name review (2026-11-02)

Read `profile-area`, `publish-dialog`, search cards and `profile-view` against H1–H10 in English and
Russian. A field for a name invites people to think it proves identity (H2, severity 2): the hint
says anyone who finds the profile sees it and that it proves nothing, and the label says it is
optional. The publish list names it first, so the user sees it before consenting (H1). A card or
view title with the name alone would invite trust (H5, severity 3): both carry the mark and short
Peer ID, the view keeps the full ID, and the facts list does not repeat the name unmarked. The mark
follows a confirmed or removed verification at once (H1). A refused name says what to change (H9).
No open H1–H10 finding.

### Contact names review (2026-11-02)

Read the chat list, header, messages, forward list and `chat-rename-dialog` against H1–H10 in English
and Russian. A public name is the peer's own claim, so `Bank Support` alone would invite trust (H2,
H5, severity 3): it always carries its mark and short Peer ID, and only a name the user gave stands
alone. The Russian mark "Проверен" is the masculine short form, which matches the existing "Контакт
ещё не проверен" because it describes the contact, not the person. A refused name would have shown
a raw error (H9, severity 2): it now says what to change, and the dialog keeps the text. Removing a
name had no visible way (H3, severity 2): the hint says an empty field removes it. Saving updates
the row and header at once (H1), and the hint says the name stays private (H2). No open H1–H10
finding.

### Never-verified notice review (2026-11-02)

Read `chat-new-device` and `chats.newDevice` against H1–H10 in English and Russian for a contact
that was never verified. The old text said a verified mark was removed, which is false for such a
contact (H1, H2, severity 3): the text now says the person is not marked verified, which holds for
both cases, and "again" went from the next step. A notice on every profile read would train users
to dismiss it (H8, severity 3): each change raises one notice, and a first contact and an older
profile raise none. A legitimate link still raises it, because only the contact can tell the two
apart, so the text says what to do unless they told you (H9). No open H1–H10 finding.

### Read mark review (2026-11-02)

Read `chats.delivery.read` against H1–H10 in English and Russian; it reuses the delivery caption,
so the smoke audits it at 440 and 1280 px with the chat detail. A chat left open behind a hidden
window would tell the peer it was read (H1, severity 4, a false claim to another person): a hidden
window neither marks read nor sends a receipt. An old receipt on a text edited later would claim
the peer saw the new wording (H1, severity 3): edited and deleted texts show no Read. A missing
label could read as "not read" (H2, severity 2): Read replaces the delivery label only when a
receipt exists, and the docs state that absence says nothing. A receipt that fails stays silent
and retries, because the user did nothing to act on (H9). A message request sends nothing (H5). No
open H1–H10 finding.

### Presence mark review (2026-11-02)

Read `chats.active` against H1–H10 in English and Russian; the smoke audits it at 440 and 1280 px
with the chat detail. A mark drawn once would stay on a Chats list left open and claim activity
that ended hours ago (H1, severity 3): every background pass now redraws a changed list, keeping
focus on an unchanged one. The prototype's Russian string named one gender for any peer (H2,
severity 2): it is now neutral, "Недавно в приложении". The mark is text, not a colored dot, so it
needs no color to read (H6, and WCAG 1.4.1). Its absence says nothing, and it shows no time, which
is the coarseness the design asks for (H2). A send that fails stays silent and retries on the next
focus, because the user did nothing to act on (H9). No open H1–H10 finding.

### Automatic photo download review (2026-11-02)

Scope: Settings → Photos and the automatic fetch in an open chat, in EN and RU.

1. Visibility of system status: an automatic fetch shows the same Downloading… line as a tap.
2. Match with the real world: the choices name the situation ("metered or unknown"), not a policy.
3. User control: Only when I ask turns it off, and it saves at once like Privacy.
4. Consistency: the card, radios and error line follow Settings → Privacy.
5. Error prevention: people not accepted never download by themselves, in any mode.
6. Recognition: all three choices are visible with the selected one marked.
7. Flexibility: a tap still works in every mode.
8. Minimalism: one question, one hint.
9. Error recovery: a failed automatic fetch keeps the Download button; a failed save shows the
   storage error and the next change retries.
10. Help: the hint says what happens after 7 days and that originals are not saved here.

No open findings.

### Chat photo original review (2026-11-02)

Scope: Download original in a chat bubble, its progress and three outcomes, in EN and RU.

1. Visibility of system status: Downloading… replaces silence, and the button is disabled for the
   fetch. The status line is a live region.
2. Match with the real world: "No longer available" says what happened without node terms.
3. User control: leaving the chat drops the result; a late answer draws nothing.
4. Consistency: the same quiet button and caption as Show photo.
5. Error prevention: an expired photo offers no download, and a blurred one offers it only after
   Show photo, so the tap order follows what the user can already see.
6. Recognition: each outcome is written in the bubble it belongs to.
7. Flexibility: a keyboard reaches the button in reading order.
8. Minimalism: the button disappears once the original is shown or cannot return.
9. Error recovery: a network failure keeps the button and says to try again; a permanent failure
   removes it, so no retry can fail the same way.
10. Help: no help is needed beyond the status line.

No open findings.

### Chat photos review (2026-11-02)

Scope: a photo in a chat, its list row and the blur before acceptance, in EN and RU, light and dark.

1. Visibility of system status: the caption names the type and size at once, the canvas holds the
   space while decoding, and the last day of retention shows the hours left.
2. Match with the real world: "Photo" and "original expires in 5 h"; the MIME type is the only raw
   term and stays secondary in the caption.
3. User control: Show photo is one click; a redraw of the history blurs again, which is the safe
   default for a preview the user has not accepted.
4. Consistency: the caption and quiet button reuse chat styles; the list row uses the same word as
   the bubble and keeps the Forwarded prefix.
5. Error prevention: text actions are absent on a photo, so no action can fail on one.
6. Recognition: the blurred state has a visible action, not a hidden click on the image.
7. Flexibility: Show photo is a normal button; focus moves to the preview, not to the page start.
8. Minimalism: a list row carries no preview, only the word.
9. Error recovery: an undecodable preview falls back to the caption with no error to dismiss.
10. Help: the caption states the time left, so no separate explanation is needed.

No open findings.

### New mail notice review (2026-11-02)

Scope: the desktop notice when mail arrives and the window has no focus.

- Visibility of system status: a notice appears only when something waits and the window would
  not show it; a focused window already shows the chat list.
- Match with the real world: "New message" in the UI language; a like uses the same words so the
  lock screen does not reveal the kind of event.
- User control: the existing New messages and likes setting turns it off, and a later notice
  replaces the earlier one, so a long absence leaves one notice.
- Error prevention: no name or message text can reach the lock screen by default.

Findings: none open. A click does nothing yet because the notice has no action; opening the app
from it is planned with the choice of notice content, and the tray and the launcher still open it.

### Sending a chat photo review (2026-11-02)

Scope: the Attach photo button of an open conversation and its failure states.

- Visibility of system status: the composer status shows Sending while the photo is prepared and
  sent, and the photo then appears in the chat with its delivery mark.
- Match with the real world: the system file picker, filtered to JPEG and PNG.
- User control: the picker's Cancel sends nothing. A sent photo has no Delete yet, which matches
  the existing rule that a photo offers no text actions; removing one is a separate feature.
- Consistency: the same quiet button, status line and queued result as a text send.
- Error prevention: the button appears only where a photo can be sent, so a request never offers
  it, and photos keep no metadata without the user having to think about it.
- Help with errors: too many waiting, too large, wrong format and no connection each say what
  happened; a draft in the box survives every outcome.

Findings: none open.

### Accepted exceptions

None.

## When to review

Before any change to the prototype or to a client screen, re-check the changed states and their
neighbours against H1–H10 in both languages at 440 and 1280 px, and record each finding here
with its fix. A change ships with no open finding of any severity (1-4). The only way past a
finding is an exception the maintainer accepted, recorded here with the reason and the date;
an agent never accepts one itself. This is the heuristics part of practice
[D3](../development/practices.md); the human approval part is not adopted yet.
