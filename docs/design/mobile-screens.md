# Mobile screens

The Android prototype follows the category hub in
[desktop screens](desktop-screens.md#settings-and-removal), with separate collection screens,
immediate switches, a catalogue-driven language picker and a numeric media-cache budget.
Native category/collection, language-menu and download-setting adoption remain planned.

Android chat attachments now use core-ordered mixed history. Photo metadata remains
visible while stranger previews are blurred; Show preview reveals only the preview,
and Download photo explicitly requests the original. Open conversations apply core
auto-download permission again after network changes. Loading disables repeat taps;
missing originals and retryable errors have separate messages. Layout uses clamped
claims until a bounded original supplies real dimensions. The last-day retention
hint is approximate and does not prevent opening a kept copy. Download-setting
controls remain planned; these native states are not in the static prototype.
Sending is now available in open chats through a system picker and explicit preview
confirmation. Refused uploads retain prepared bytes for retry; queued sends clear
them. Picker composition is keyed by identity and destination chat.

Android implementation checkpoint: a loaded profile offers **Share profile…**.
The native destination dialog lists open chats except the profile owner, using
contact names where available. It explains an empty list, prevents repeat sends
while busy, keeps choices after a failed send, and distinguishes queued delivery
from success. After either stored outcome the user can open the destination chat.
Cancel before selection sends nothing. This native dialog is not represented in
the static prototype yet. Exact profile links in live messages render as buttons;
only a press loads the verified profile, whose Back returns to the original chat.

Control IDs and states for every screen of the Android client. The static prototype in
`clients/android/prototype/` is the layout reference: Material 3 phone layout, light and dark,
every state below, no logic, not shipped. It reuses the desktop prototype wording
(`clients/desktop/prototype/strings.js`) and overrides only the Android lines in its own
`strings.js`. `make ui-prototype-android` saves every state in English and Russian, light and
dark, at 360 and 412 px and normal/200% text-only zoom to
`build/ux-research/prototype-android/<state>[-dark][--text200]-<lang>-<width>.png`
([UI testing](../testing/ui-testing.md#android-prototype-screenshots)).
[v0.0.1 client screens](v0.0.1-client-screens.md) holds the behaviour contracts;
[desktop screens](desktop-screens.md) the same IDs for the desktop shell.

The Android client follows this prototype, and its test tags are the IDs below. The sections below
note every difference from the prototype. `make quality` checks that every ID and state in backticks on
this page is an id or a data-on state in the prototype. The prototype wording is a draft; the shipped strings stay in
`clients/android/app/src/main/res`.

## Layout

In the static prototype, top/back bars and navigation grow with enlarged text instead of clipping
it. Navigation labels and unread badges remain complete; distinct controls, hints and identity-sheet
rows retain the spacing required by the [review criteria](review-criteria.md#layout-l1-l2).
All static code values use one shared document-wide identifier formatter, preserving raw copied
text; hidden states take precedence over its presentation. Picker, search and chat lists separate
rows and status/actions, and message actions wrap with the existing spacing tokens.
Profile birth fields use the adaptive desktop grid pattern, keeping values readable as fields
move onto separate rows. The relationship-goal field reuses the shared wrapping choice dialog;
`profile-goal-dialog` shows it open. Its labelled opener displays the complete selected value,
and the hidden `profile-goal` select retains the value and choice semantics.
The native birth row has not adopted this reflow yet; enlarged-font Compose verification remains
planned.

Phones get one screen at a time. Five top-level sections sit in a bottom navigation bar; each has
a top app bar with its title, the connection status and the identity avatar. Detail
screens (a found profile, a chat, the app lock settings) replace both bars with a back bar.
Dialogs are Material dialogs over a scrim; the identity switcher is a bottom sheet.
In the static prototype, scrims cover the viewport and scroll independently of the background.
Oversized dialogs and sheets start at a reachable edge so their text and final actions can be
scrolled into view at enlarged text sizes.

| Control | ID | Notes |
| --- | --- | --- |
| Profile | `nav-profile` | Bottom bar; the start section |
| Search | `nav-search` | |
| Chats | `nav-chats` | Badge `chats-unread`: unread messages plus message requests, a request counting once |
| Network | `nav-network` | |
| Settings | `nav-settings` | |
| Connection status | `connection-status` | Top bar chip; opens Network |
| Identity avatar | `identity-switcher` | Top bar; opens `switcher` |

## App lock

`lock-setup`: the first launch, before any identity. A PIN of 6 to 64 digits, or device
biometrics; the device screen lock recovers a forgotten PIN.

| Control | ID | Notes |
| --- | --- | --- |
| PIN, repeated | `pin-set`, `pin-repeat` | On the page above the actions; the prototype omits them |
| Set PIN | `lock-set` | Primary |
| Use device biometrics | `lock-biometric` | Only when enrolled |
| Skip | `lock-skip` | Text button |

`lock-unlock`: every fresh start while a PIN is set.

| Control | ID | Notes |
| --- | --- | --- |
| PIN | `pin-unlock` | Numeric |
| Wrong PIN | `lock-error` | Shown after a wrong PIN |
| Unlock | `lock-unlock` | Primary |
| Use device screen lock | `lock-device` | Always offered; the primary action once too many wrong PINs require it |

`lock-settings`: from the Settings row `lock-settings`, or the last action on `picker` and
`recovery`, the only path there while no identity opens (not in the prototype); back with
`lock-back` or the system back, to Settings when an identity is open.

| Control | ID | Notes |
| --- | --- | --- |
| Current PIN | `pin-current` | Needed for both actions |
| New PIN, repeated | `pin-set`, `pin-repeat` | |
| Change PIN | `lock-change` | Primary |
| Turn off PIN | `lock-turn-off` | Danger text button |

## First run

`wizard-identity`: step 1 of 2. Android keeps the key in Android Keystore, so there is no
storage choice; `key-store` says so and `no-backup` says there is no backup. Nothing is
created before `create-identity`.

`wizard-network`: step 2 of 2, may be skipped: `peer-id`, `copy-peer-id`, `network-name`,
`seeds`, `skip-network`, `check-connection`, as on the desktop. Beyond the prototype, Android
checks and activates in two steps: a connected check replaces the primary action with Continue,
which activates the network, and a second Continue leaves the wizard. It also keeps Save draft and
a draft status line for leaving the settings unchecked.

`picker`: several identities and none chosen, the list `identities` of `pick-1`, `pick-2` by
short Peer ID, and `add-identity`.
`recovery`: the chosen identity cannot be opened (`recovery-reason`, `recovery-details`,
`copy-recovery-details`, `recovery-peer`); `retry`, `switch-identity` and `remove-identity`.
Nothing is created in its place.

## Sections

The IDs inside each section are the desktop ones; only the frame differs.

- Profile, `profile` and `profile-hidden`: the form (`profile-hidden`, `profile-name`,
  `profile-gender`, `profile-goal`, `birth-year`, `birth-month`, `birth-day`, `profile-country`,
  `profile-place`, `profile-more`), `profile-save` and `profile-publish`. The optional public
  name is trimmed on save, and a refused one leaves the saved draft unchanged. `publish-dialog` and
  `publish-progress` show `profile-public-fields`, `profile-attested`, `profile-disclosure`,
  `profile-cancel` and `profile-publish-confirm`. On Android the `profile-hidden` switch
  turns on once a profile was published and needs an answering node; off opens the publish dialog
  and stays on until the publication succeeds, as on desktop. Its progress state shows the saved proof nonce
  instead of the elapsed time, and the signed readback appears on the form after the dialog closes.
- Network, `network`, `network-checking` and `network-error`: `network-name-field`,
  `network-seeds`, `seed-rows`, `check-and-connect`, `cancel-check`, `network-error-details`.
  One action checks and, when a node answered, activates; a failed or cancelled check keeps the
  previous network. Android's status line names the active network, and its row texts say why a
  row failed in more detail than the prototype's.
- Search, `search`, `search-loading`, `search-empty` and `search-error`: `search-age-min`,
  `search-age-max`, `search-country`, `search-place`, `search-more`, `search-employer-any`,
  `search-employer-same`, `search-employer-exclude`, `search-company`, `search-employer-notice`,
  `run-search`,
  `search-results` with `result-1` and `result-2`, `search-again`, `search-cancel`,
  `search-failures`, `search-open-network`, `search-retry`. Android shows filters only for the
  fields the nodes offered, so a first sample comes before them; the employer filter runs on the
  device after verification, so it is always shown, with the notice that it cannot hide the user's
  own profile from colleagues; a result shows the short Peer ID,
  possible age and country, and opens the found profile; Cancel abandons the answer while the node
  call runs to its end; Open Network and Retry sit under the failure notice and hide for an age no birth
  date fits; the intro also says that profile fields are self-stated, which the prototype omits.
- Chats, `chats`: `chats-liked`, `chats-requests`, `chat-list` with `chat-1` and `chat-2`, and
  `chat-muted` on a muted peer's row. On
  Android both top rows always show, with a count from local data, and expand their lists in place.
  Liked you fetches the verified profiles when it opens and each opens the found profile, so a peer
  no node returns counts but has no row. Message requests stay out of the chat list and open the
  chat. A chat row and the chat header name the peer by the desktop rule
  ([contact names](desktop-screens.md)): the local name; else the public name, Verified or Not
  verified and the short Peer ID; else the short Peer ID; with the main photo once fetched. The
  row opens the chat. `chat-rename` in the chat menu names the peer on this device. On iOS and macOS
  the menu also opens a sheet with the safety number and QR code, without a copy, a scan or a
  verified mark. The app receives
  the mailbox every minute while it is in the foreground, on every section and in a chat, and a
  background job about every 15 minutes otherwise ([privacy](../security/privacy.md#local-notices)); failures
  stay quiet.
- Linking a phone to an identity and the Devices list are planned; the desktop screens define the
  flow, see [desktop-screens.md](desktop-screens.md#linking-this-computer).
- Settings, `settings`: a fixed category hub, with no identity/peer rows or destructive actions.
  Categories are `settings-appearance`, `settings-privacy`, `settings-notifications`,
  `settings-photos`, `settings-account` and `settings-about`. Each has a
  back bar; detail categories hide the shell bars. Hash links demonstrate navigation only.
  Appearance retains `theme` (`theme-system`, `theme-light`, `theme-dark`) and the
  `language` select. Its shared wrapping picker is captured as `settings-language-dialog`:
  the system default precedes every registered catalogue, named by its own language name.
  Cancel/Escape preserve the locale code and restore focus; this static demonstration does
  not change the page locale or persist the choice.
- Privacy contains immediate switches `send-views`, `send-read-receipts`, `send-typing`
  and the `presence-audience` radios. Its hint scopes these preferences to this identity on
  this device. Separate count links open `settings-blocked` and `settings-passed`.
  The distinct `lock-settings` row opens app lock, whose scope is the whole app.
  Notifications has `message-notifications` and `notice-content` (`notice-fixed`,
  `notice-name`, `notice-text`). Photos uses `photo-mode`, the `keep-photos` switch,
  numeric MiB `photo-cache-size`, a usage hint and `photo-cache-clear`.
  Native Android now implements the download-mode radio group through the common core API;
  retention, numeric budget/usage and cache-clear controls remain planned.
- Account links to `settings-identities-screen`. Its `settings-identities` list preserves
  `settings-remove`, `settings-remove-2` and `settings-create`. Blocked and passed have
  `blocked-list` and `passed-list` in the `blocked` and `passed` cards:
  `retry-revocation-1`, `unblock-1`, `unpass-1`, `blocked-none` and `passed-none`
  retain their meanings. All three collections reuse the desktop prototype controller:
  search, Previous/Next/Refresh, live status, at most 32 mounted rows and entry counts.
  Search matches the complete local fixture text and resets paging; Refresh restores fixtures.
  A stale completion after navigation cannot replace a different view. Rows wrap their complete
  raw identifiers and action labels; the shared identifier formatter leaves copied text unchanged.
- Android remains a light client, with no embedded store/search node controls
  ([ADR 0030](../decisions/0030-desktop-full-node.md)).
  About retains the version and disabled `updates` placeholder: `update-6h`,
  `update-12h`, `update-24h`, `update-7d`, `update-state` and `update-check`.
  It sends no request and saves nothing.
- `remove-dialog` and `remove-offline` use the identities screen as their background.
  `remove-title`, `remove-peer`, `remove-confirm`, `remove-cancel`, `remove-action`,
  `remove-cancel-offline`, `remove-retry` and `remove-anyway` retain the removal contract.
  A published profile is withdrawn before the key goes; if no node accepts it, offer Retry,
  Remove anyway and Cancel. A key that cannot open cannot sign the withdrawal and skips it.

Android screens may be reviewed and implemented individually against the frozen criteria and the
corresponding desktop behavior. The complete prototype review and per-client impact review remain
v0.0.1 release requirements; this delivery order does not waive criteria or native verification.

Native Android profile pages include a private Save/Remove action and a private note editor,
scoped to the current identity and profile opening. Both use common-core local operations even if
the public profile cannot be loaded. Search's saved-profile collection reads local IDs/cached labels;
only an explicit row Open enters the ordinary verified profile-loading route. Note load failures
stay distinct from empty text, failed saves retain the draft, and stale presentation completions
cannot update another opening. Notes never enter public profile fields, likes or chat content.

The native Settings route/frame components implement six categories, per-identity route ownership,
collection-to-category Back and a saved presentation-only route across the app-lock overlay.
Their integration into the shell and existing panels is being completed; these components alone
are not native screen acceptance.

Native Android Settings still shows its original cards: identity/removal, app lock, blocked/passed,
appearance, privacy/notifications, photo download mode, disabled updates and version. The receipt switch defaults off;
notice delivery defaults on with fixed “New message” content. Turning delivery on requests
notification permission on Android 13+; when blocked, Allow asks again or opens system settings.
Content selection is disabled while off and retains its choice. Failed reads/writes preserve the
last shown values/list and explain the failure. Token-revocation retry reports its outcome.
The catalogue-driven menu, category hub, separate paged/searchable collections and media controls
are not established as native behavior by these prototype captures.

The theme and language follow the system by default; the prototype shows the dark theme with
`?theme=dark` instead of a separate state. On Android a picked theme applies at once. A picked
language relaunches the activity back into Settings; with an app lock on, that asks to unlock
first and returns to the first section, as any activity relaunch does.

## Detail screens

- Found profile, `profile-view`, `profile-view-match` and `unlike-dialog`: `view-back`,
  `view-photos`, `view-peer`, `view-like`, `view-unlike`, `view-message`, `view-match`,
  `view-fields`, `view-pass`, `view-unpass`, `view-block`; `unlike-confirm` and `unlike-cancel` in
  the dialog. Android has no `view-photos` yet. Opened from a chat, or
  after the app was recreated, the page fetches the profile from the network and offers a retry; it
  hides `view-like`, `view-message`, `view-pass` and `view-block` on the own profile. A failed like that stayed
  staged says it goes out when a node answers; any other shows why, and the stored likes decide the
  button. `view-pass` sits below the profile and is not offered while a like stands; a pass hides
  `view-like` until `view-unpass` undoes it, and the peer is not told. `view-block` sits beside it,
  asks first in the dialog the chat menu uses, then leaves for Chats, whose note says the outcome.
- Chat, `chat`, `chat-closed`, `chat-first`, `chat-request`, `chat-menu`, `chat-clear-dialog`,
  `chat-cleared` and `delete-dialog`: `chat-back`, `chat-profile`, `chat-more` with
  `chat-clear`, `chat-mute` and `chat-block`, `chat-thread`, `msg-menu` (long press on an own
  message),
  `chat-input`, `chat-send`, `request-block`; `chat-clear-confirm` and `delete-confirm` in the
  dialogs. Android has `chat`, `chat-closed`, `chat-first` and `chat-request` with `chat-back`,
  `chat-profile`, `chat-first-notice`, `chat-closed-notice`, `chat-request-notice`, `chat-thread`,
  `chat-input` and `chat-send`, plus `chat-more` with `chat-rename`, `chat-clear`, `chat-mute` and
  `chat-block`, `chat-clear-dialog` and the `chat-cleared-note` above the thread after a clear.
  The static Android prototype uses the shared desktop catalogue: `clear.text` explains local
  removal and the peer's copy; `clear.pending` explains cancellation and saved packs, before
  the unchanged `clear.later` warning. Android overrides `clear.text` and `chat.cleared` to say
  this device rather than this computer, matching its native wording. Dialog titles wrap long words
  at enlarged text sizes;
  the chat header lets the contact shrink so its navigation controls stay inside the viewport.
  Clear names the contact in `chat-clear-name`, then shows the full `chat-clear-peer` under
  `chat-clear-peer-label`, before the consequences. The name and ID come from the same header
  fixture; the shared identifier renderer chunks the ID without changing copied text. Centered
  dialogs fall back to start alignment when taller than their available space, keeping the title
  reachable with enlarged text.
  `chat-rename` opens `chat-rename-dialog`: `chat-rename-input` starts from the local name, an
  empty one removes it, `chat-rename-save` saves and every row and the header read it again,
  `chat-rename-cancel` closes, and a line in the dialog says why a name was refused. A request hides
  `chat-block` and offers `request-block` on its notice instead. Unlike the desktop, a block asks
  first in a confirmation dialog, because the chat closes and Unblock sits in Settings; after a
  block the chat closes and a note on Chats says the outcome, with the details when the revocation
  is still pending. Opening a chat
  marks its texts read and, with `send-read-receipts` on in an accepted chat, sends the read receipts it owes while it stays on screen. A long press on an own message that is not
  deleted opens `msg-menu` with Edit, while the chat takes text, and `msg-delete`. Edit loads the
  text into `chat-input` under a line that says an edit is open, with a cancel that gives back
  the text typed before it; Send saves the edit and stays off for an unchanged text. Delete asks
  in `delete-dialog` with `delete-confirm`. A change stored on the device whose delivery failed
  says it is pending, and a failure says why. `chat-mute` reads Mute or Unmute notifications and
  hides while the peer has no chat; on Android it keeps the peer out of the new-mail notice,
  and the owner's other devices get the flag too. It opens from a Chats row and from `view-message` on
  the found profile, back always lands on Chats,
  and `chat-profile` opens the found profile. The first-message notice also shows before that
  message, and an own message reads Pending until a node takes it, Sent, then Delivered, and Read once the peer's read receipt covers it.

## Identity switcher

`switcher`: a bottom sheet `identity-menu` over the current section with the active identity,
`switch-to-2` and `add-identity-menu`. There is no sidebar, so the desktop menu state has
no Android counterpart.
