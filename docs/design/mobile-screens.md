# Mobile screens

Control IDs and states for every screen of the Android client. The static prototype in
`clients/android/prototype/` is the layout reference: Material 3 phone layout, light and dark,
every state below, no logic, not shipped. It reuses the desktop prototype wording
(`clients/desktop/prototype/strings.js`) and overrides only the Android lines in its own
`strings.js`. `make ui-prototype-android` saves every state in English and Russian, light and
dark, at 360 and 412 px to `build/ux-research/prototype-android/<state>[-dark]-<lang>-<width>.png`
([UI testing](../testing/ui-testing.md#android-prototype-screenshots)).
[v0.0.1 client screens](v0.0.1-client-screens.md) holds the behaviour contracts;
[desktop screens](desktop-screens.md) the same IDs for the desktop shell.

The Android client follows this prototype, and its test tags are the IDs below. The sections below
note every difference from the prototype. `make quality` checks that every ID and state in backticks on
this page is an id or a data-on state in the prototype. The prototype wording is a draft; the shipped strings stay in
`clients/android/app/src/main/res`.

## Layout

Phones get one screen at a time. Five top-level sections sit in a bottom navigation bar; each has
a top app bar with its title, the connection status and the identity avatar. Detail
screens (a found profile, a chat, the app lock settings) replace both bars with a back bar.
Dialogs are Material dialogs over a scrim; the identity switcher is a bottom sheet.

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

- Profile, `profile` and `profile-hidden`: the form (`profile-hidden`, `profile-gender`,
  `profile-goal`, `birth-year`, `birth-month`, `birth-day`, `profile-country`, `profile-place`,
  `profile-more`), `profile-save` and `profile-publish`. `publish-dialog` and
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
  `search-age-max`, `search-country`, `search-place`, `search-more`, `run-search`,
  `search-results` with `result-1` and `result-2`, `search-again`, `search-cancel`,
  `search-failures`, `search-open-network`, `search-retry`. Android shows filters only for the
  fields the nodes offered, so a first sample comes before them; a result shows the short Peer ID,
  possible age and country, and opens the found profile; Cancel abandons the answer while the node
  call runs to its end; Open Network and Retry sit under the failure notice and hide for an age no birth
  date fits; the intro also says that profile fields are self-stated, which the prototype omits.
- Chats, `chats`: `chats-liked`, `chats-requests`, `chat-list` with `chat-1` and `chat-2`. On
  Android both top rows always show, with a count from local data, and expand their lists in place.
  Liked you fetches the verified profiles when it opens and each opens the found profile, so a peer
  no node returns counts but has no row. Message requests stay out of the chat list and open the
  chat. A chat row shows the short Peer ID rather than a name and opens the chat. The app receives
  the mailbox every minute while it is in the foreground, on every section and in a chat; failures
  stay quiet.
- Settings, `settings`: `settings-identities` (`settings-remove`, `settings-remove-2`,
  `settings-create`), the `lock-settings` row, `blocked` (`unblock-1`), `theme` (`theme-system`,
  `theme-light`, `theme-dark`) and `language` (`language-system`, `language-en`, `language-ru`),
  `privacy` (`send-views`, `send-read-receipts`, `send-typing`, `presence-audience`,
  `message-notifications`) and `node` (`node-enabled`). `remove-dialog` and `remove-offline`
  confirm a removal with `remove-title`, `remove-peer`, `remove-confirm`, `remove-cancel`,
  `remove-action`, `remove-cancel-offline`, `remove-retry` and `remove-anyway`. A published profile
  is withdrawn from search before the key goes; when no node takes the withdrawal, `remove-offline`
  offers Retry, Remove anyway and Cancel. A key that does not open cannot sign a withdrawal, so
  removing that identity skips it. Android Settings has the identity rows, the lock
  row, the theme and language, and the version; a block list, privacy and node settings have no
  Android feature yet and are planned. It switches identities only in the identity menu.

The theme and language follow the system by default; the prototype shows the dark theme with
`?theme=dark` instead of a separate state. On Android a picked theme applies at once. A picked
language relaunches the activity back into Settings; with an app lock on, that asks to unlock
first and returns to the first section, as any activity relaunch does.

## Detail screens

- Found profile, `profile-view`, `profile-view-match` and `unlike-dialog`: `view-back`,
  `view-photos`, `view-peer`, `view-like`, `view-unlike`, `view-message`, `view-match`,
  `view-fields`, `view-block`; `unlike-confirm` and `unlike-cancel` in the dialog. Android has no
  `view-photos` yet and `view-block` is planned. Opened from a chat, or after the app was recreated,
  the page fetches the profile from the network and offers a retry; it hides `view-like` and
  `view-message` on the own profile. A failed like shows why, then the stored likes decide the
  button.
- Chat, `chat`, `chat-closed`, `chat-first`, `chat-request`, `chat-menu`, `chat-clear-dialog`,
  `chat-cleared` and `delete-dialog`: `chat-back`, `chat-profile`, `chat-more` with
  `chat-clear` and `chat-block`, `chat-thread`, `msg-menu` (long press on an own message),
  `chat-input`, `chat-send`, `request-block`; `chat-clear-confirm` and `delete-confirm` in the
  dialogs. Android has `chat`, `chat-closed`, `chat-first` and `chat-request` with `chat-back`,
  `chat-profile`, `chat-first-notice`, `chat-closed-notice`, `chat-request-notice`, `chat-thread`,
  `chat-input` and `chat-send`; the
  menus, dialogs and blocking are planned, so the request notice does not offer a block yet. It
  opens from a Chats row and from `view-message` on the found profile, back always lands on Chats,
  and `chat-profile` opens the found profile. The first-message notice also shows before that
  message, and an own message reads Pending until a node takes it, Sent, then Delivered.

## Identity switcher

`switcher`: a bottom sheet `identity-menu` over the current section with the active identity,
`switch-to-2` and `add-identity-menu`. There is no sidebar, so the desktop menu state has
no Android counterpart.
