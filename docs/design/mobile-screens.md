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

The Android client today has its own test tags and a single-column layout. Relayouting it to
this prototype renames the tags to the IDs below; until then this page is the plan, not the
shipped state. `make quality` checks that every ID and state in backticks on this page is an id
or a data-on state in the prototype. The prototype wording is a draft; the shipped strings stay
in `clients/android/app/src/main/res`.

## Layout

Phones get one screen at a time. Five top-level sections sit in a bottom navigation bar; each
has a top app bar with its title, the connection status and the identity avatar. Detail
screens (a found profile, a chat, the app lock settings) replace both bars with a back bar.
Dialogs are Material dialogs over a scrim; the identity switcher is a bottom sheet.

| Control | ID | Notes |
| --- | --- | --- |
| Profile | `nav-profile` | Bottom bar; the start section |
| Search | `nav-search` | |
| Chats | `nav-chats` | Unread badge `chats-unread` |
| Network | `nav-network` | |
| Settings | `nav-settings` | |
| Connection status | `connection-status` | Top bar chip; opens Network |
| Identity avatar | `identity-switcher` | Top bar; opens `switcher` |

## App lock

`lock-setup`: the first launch, before any identity. A PIN of 6 to 64 digits, or device
biometrics; the device screen lock recovers a forgotten PIN.

| Control | ID | Notes |
| --- | --- | --- |
| Set PIN | `lock-set` | Primary |
| Use device biometrics | `lock-biometric` | Only when enrolled |
| Skip | `lock-skip` | Text button |

`lock-unlock`: every fresh start while a PIN is set.

| Control | ID | Notes |
| --- | --- | --- |
| PIN | `pin-unlock` | Numeric |
| Wrong PIN | `lock-error` | Shown after a wrong PIN |
| Unlock | `lock-unlock` | Primary |
| Use device screen lock | `lock-device` | Always offered |

`lock-settings`: from the Settings row `lock-settings`; back with `lock-back`.

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
`seeds`, `skip-network`, `check-connection`, as on the desktop.

`picker`: several identities and none chosen, `pick-1`, `pick-2` and `add-identity`.
`recovery`: the chosen identity cannot be opened (`recovery-reason`, `recovery-details`,
`copy-recovery-details`, `recovery-peer`); `retry`, `switch-identity` and `remove-identity`.
Nothing is created in its place.

## Sections

The IDs inside each section are the desktop ones; only the frame differs.

- Profile, `profile` and `profile-hidden`: the form (`profile-hidden`, `profile-gender`,
  `profile-goal`, `birth-year`, `birth-month`, `birth-day`, `profile-country`, `profile-place`,
  `profile-more`), `profile-save` and `profile-publish`. `publish-dialog` and
  `publish-progress` show `profile-public-fields`, `profile-attested`, `profile-disclosure`,
  `profile-cancel` and `profile-publish-confirm`.
- Network, `network`, `network-checking` and `network-error`: `network-name-field`,
  `network-seeds`, `seed-rows`, `check-and-connect`, `cancel-check`, `network-error-details`.
- Search, `search`, `search-loading`, `search-empty` and `search-error`: `search-age-min`,
  `search-age-max`, `search-country`, `search-place`, `search-more`, `run-search`,
  `search-results` with `result-1` and `result-2`, `search-again`, `search-cancel`,
  `search-failures`, `search-open-network`, `search-retry`.
- Chats, `chats`: `chats-liked`, `chats-requests`, `chat-list` with `chat-1` and `chat-2`.
- Settings, `settings`: `settings-identities` (`settings-remove`, `settings-remove-2`,
  `settings-create`), the `lock-settings` row, `blocked` (`unblock-1`), `theme` and `language`,
  `privacy` (`send-views`, `send-read-receipts`, `send-typing`, `presence-audience`,
  `message-notifications`) and `node` (`node-enabled`). `remove-dialog` and `remove-offline`
  confirm a removal with `remove-confirm`, `remove-action`, `remove-retry` and `remove-anyway`.

The theme follows the system by default; the prototype shows the dark theme with
`?theme=dark` instead of a separate state.

## Detail screens

- Found profile, `profile-view`, `profile-view-match` and `unlike-dialog`: `view-back`,
  `view-photos`, `view-peer`, `view-like`, `view-unlike`, `view-message`, `view-match`,
  `view-fields`, `view-block`; `unlike-confirm` and `unlike-cancel` in the dialog.
- Chat, `chat`, `chat-first`, `chat-request`, `chat-menu`, `chat-clear-dialog`,
  `chat-cleared` and `delete-dialog`: `chat-back`, `chat-profile`, `chat-more` with
  `chat-clear` and `chat-block`, `chat-thread`, `msg-menu` (long press on an own message),
  `chat-input`, `chat-send`, `request-block`; `chat-clear-confirm` and `delete-confirm` in the
  dialogs.

## Identity switcher

`switcher`: a bottom sheet `identity-menu` over the current section with the active identity,
`switch-to-2` and `add-identity-menu`. There is no sidebar, so the desktop menu state has
no Android counterpart.
