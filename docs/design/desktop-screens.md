# Desktop screens

Control IDs and states for every screen of the Linux and Windows shell in
[ADR 0033](../decisions/0033-desktop-navigation-shell.md). The implementation and its UI tests use
these IDs; IDs already in the current client keep their meaning. The static prototype in
`clients/desktop/prototype/` is the layout reference: it shows every state with the shipped
stylesheet `clients/desktop/ui/style.css`, is not shipped itself and has no logic. `make ui-prototype` saves every state in English and
Russian at 440 and 1280 px to `build/ux-research/prototype/<state>-<lang>-<width>.png`
([UI testing](../testing/ui-testing.md#desktop-prototype-screenshots)). The client already has the
shell with the Profile, Search, Network and Settings areas and both first-run steps; chats and
the switcher are planned. [v0.0.1 client screens](v0.0.1-client-screens.md) holds the
behaviour contracts.

`make quality` checks that every ID and state in backticks on this page is an id or a
data-on state in the prototype, so the page and the prototype cannot drift apart unnoticed.
The prototype wording is a draft; the shipped strings stay in `clients/desktop/ui/i18n`.

## First run

`wizard-identity`: step 1 of 2. The system keyring is preselected when available; the file option
says the key is kept unencrypted. Nothing is created before **Create identity**.

| Control | ID | Notes |
| --- | --- | --- |
| System keyring | `system-choice` | Radio, preselected when available |
| File in the app folder | `file-choice` | Radio; says the key is unencrypted |
| No-backup line | `no-backup` | Calm text, no reminder later |
| Create identity | `create-identity` | Primary |

`wizard-network`: step 2 of 2, may be skipped. Same form and outcome as the Network screen.

| Control | ID | Notes |
| --- | --- | --- |
| Peer ID | `peer-id` | Full ID |
| Copy | `copy-peer-id` | |
| Network name | `network-name` | Placeholder "Public network" |
| Bootstrap addresses | `seeds` | One host:port per line |
| Skip for now | `skip-network` | Left |
| Check and connect | `check-connection` | Primary, right |

## Picker and recovery

`picker`: several identities and none active, or after a removal. Every identity list (picker,
switcher, Settings) leads each row with a round avatar: the thumbnail of the identity's first
profile photo, or the two characters after "@" in the short Peer ID when it has none. A short Peer
ID is "@" and the last 6 characters, longer until unique among the identity's known peers; the
full ID shows in the wizard, recovery and removal. Opened from an open identity, the picker and
the key-storage choice also show Cancel, which returns to that identity.

| Control | ID | Notes |
| --- | --- | --- |
| Identity rows | `pick-1`, `pick-2` | Avatar, short Peer ID, storage kind |
| Add identity | `add-identity` | |

`recovery`: the active identity cannot be opened; the localized reason, then Details with the
state code.

| Control | ID | Notes |
| --- | --- | --- |
| Reason | `recovery-reason` | Localized sentence |
| Details | `recovery-details` | Collapsed; code and row |
| Copy details | `copy-recovery-details` | |
| Peer ID | `recovery-peer` | Full ID |
| Remove | `remove-identity` | "Remove this identity…", a quiet link under the Peer ID; opens the removal dialog |
| Use another identity | `switch-identity` | |
| Retry | `retry` | Primary |

## Shell

Every later screen sits in the shell. `switcher` opens the identity menu; `menu` is the
off-canvas sidebar at 640 px and below, opened by `menu-open` in the top bar.

| Control | ID | Notes |
| --- | --- | --- |
| Open menu | `menu-open` | Top bar, 640 px and below |
| Active identity | `identity-switcher` | Avatar, short Peer ID, storage kind |
| Identity menu | `identity-menu` | Other identities and Add identity |
| Other identity | `switch-to-2` | |
| Add identity | `add-identity-menu` | |
| Profile | `nav-profile` | `aria-current="page"` when selected |
| Search | `nav-search` | |
| Chats | `nav-chats` | `chats-unread`: unread messages plus message requests |
| Network | `nav-network` | |
| Settings | `nav-settings` | Pinned last |
| Connection status | `connection-status` | Connecting, Connected or Offline; opens Network |

Connected means one compatible node answered. The top bar repeats the status as a word at 640 px
and below. `search-error` shows the offline shell; a failed check in `network-error` keeps the
previous network and its status.

## Profile

`profile`: the draft with **Hide from search**. `profile-hidden`: the switch is on, a tombstone is
published and a notice explains how to return. `profile-dark`: the same screen in the dark theme.

| Control | ID | Notes |
| --- | --- | --- |
| Status | `profile-status` | Draft, published with version, or hidden |
| Hide from search | `profile-hidden` | Switch |
| Hidden notice | `profile-hidden-notice` | Only while hidden |
| Gender | `profile-gender` | |
| Looking for | `profile-goal` | |
| Birth year, month, day | `birth-year`, `birth-month`, `birth-day` | Month and day optional |
| Shown age | `profile-age` | "31 or 32" for a partial date |
| Country, town | `profile-country`, `profile-place` | |
| More about you | `profile-more` | Collapsed optional fields |
| Save draft | `profile-save` | |
| Preview and publish | `profile-publish` | Primary; opens the dialog |

`publish-dialog` lists exactly the public fields and needs both boxes; `publish-progress` shows the
proof of work with elapsed time. Cancel is left and focused.

| Control | ID | Notes |
| --- | --- | --- |
| Public fields | `profile-public-fields` | Exactly what is published |
| I am 18 or older | `profile-attested` | Required |
| Public fields may be copied | `profile-disclosure` | Required |
| Cancel | `profile-cancel` | Left, focused |
| Publish | `profile-publish-confirm` | Right |

## Network

`network`, `network-checking` and `network-error`. The network name stands above the addresses,
its placeholder says "Public network" when empty. One **Check and connect**; a failure or cancel
keeps the prior network.

| Control | ID | Notes |
| --- | --- | --- |
| Status | `network-status` | |
| Error details | `network-error-details` | Collapsed; code and row outcomes; the error says nothing changed |
| Copy details | `copy-network-details` | |
| Network name | `network-name-field` | |
| Bootstrap addresses | `network-seeds` | |
| Row outcomes | `seed-rows` | Answered, Checking or No answer per row |
| Cancel | `cancel-check` | While checking |
| Check and connect | `check-and-connect` | Primary |

## Search

`search`, `search-loading`, `search-empty` and `search-error`.

| Control | ID | Notes |
| --- | --- | --- |
| Age from, to | `search-age-min`, `search-age-max` | |
| Country, town | `search-country`, `search-place` | |
| More filters | `search-more` | Collapsed |
| Search | `run-search` | Primary |
| Cancel | `search-cancel` | While searching |
| Status | `search-status` | "2 profiles" |
| Results | `search-results` | Cards `result-1`, `result-2` |
| Show another sample | `search-again` | Not in the empty state; the advice points at the filters |
| Failures | `search-failures` | Collapsed details |
| Copy details | `copy-search-details` | |
| Open Network | `search-open-network` | Offline only |
| Retry | `search-retry` | |

## Found profile

`profile-view` opens from a result card inside Search, which stays selected. Search lists load
only thumbnails; the full photos load here. Field values are claims of the owner and say so;
nothing is shown as verified. Before a match the hint says one message can be sent until a reply.
`profile-view-match`: a mutual like adds the match notice, Like becomes Liked (aria-pressed), and
pressing it opens `unlike-dialog`; confirming sends the unlike, which ends the match for both.
Block is local, hides the peer from search and chats, sits apart from Like as a quiet button under
the fields, and is undone in Settings.

| Control | ID | Notes |
| --- | --- | --- |
| Back to results | `view-back` | |
| Photos | `view-photos` | Gallery with thumbnails under it |
| Peer ID | `view-peer` | Short ID |
| Copy | `copy-view-peer` | Copies the full ID |
| Match notice | `view-match` | Match only |
| Message | `view-message` | `view-message-match` after a match |
| Like | `view-like` | `view-unlike` (Liked) after a match |
| Fields | `view-fields` | Looking for, children, interests, languages |
| Claims note | `view-claims` | Written by the owner; nobody checks it |
| Block | `view-block` | Quiet, under the fields |

`unlike-dialog` confirms taking the like back.

| Control | ID | Notes |
| --- | --- | --- |
| Cancel | `unlike-cancel` | Left, focused |
| Take back | `unlike-confirm` | Right |

## Chats

`chats`: the active identity's chats only; there is no shared inbox. **Liked you** lists
received likes; liking back makes a match. **Message requests** come from non-matches; the sender
learns nothing until a reply (read receipts are off by default). Conversations and requests show
the peer's photo thumbnail (letters only without a photo), the public name or short Peer ID, the
last message, its time and the unread count.

| Control | ID | Notes |
| --- | --- | --- |
| Liked you | `chats-liked` | With count |
| Message requests | `chats-requests` | With count |
| Conversations | `chat-list` | Rows `chat-1`, `chat-2` |

`chat`: one conversation with a match. The peer's avatar and name in the header open the found
profile, so a narrow window keeps one text button. The header shows "Active recently" when the
last signal from the peer is at most 2 hours old; there is no online or last-seen time. Own
messages show Sent or Delivered, and Read only when the peer sends read receipts; an edited
message says so, and a deleted one leaves a placeholder. Edit and Delete apply to own messages and
are best effort. `chat-first`: to a non-match, one message is sent and the composer stays disabled
until a reply or a match. `chat-request`: an incoming message from a non-match, with Reply through
the composer or Block. "Anna is typing…" shows for 10 seconds after the last typing signal, only
over a direct path.

| Control | ID | Notes |
| --- | --- | --- |
| Back | `chat-back` | |
| Peer | `chat-profile` | Opens the found profile |
| Presence | `chat-presence` | "Active recently" |
| More | `chat-more` | Opens `chat-menu` |
| Clear chat | `chat-clear` | In the menu |
| Block | `chat-block` | In the menu |
| First-message notice | `chat-first-notice` | `chat-first` only |
| Request notice | `chat-request-notice` | `chat-request` only |
| Block | `request-block` | In the request notice |
| Message actions | `msg-menu` | Edit and Delete on own messages |
| Delete | `msg-delete` | Opens `delete-dialog` |
| Typing | `chat-typing` | |
| Message | `chat-input` | |
| Send | `chat-send` | Disabled while the box is empty |

`chat-clear-dialog` confirms Clear chat: the messages are deleted from this computer only, the
peer keeps theirs, cleared messages do not come back and unsent ones are cancelled
([contract](message-edits-and-deletion.md#clearing-a-conversation-planned)). `chat-cleared`: the
same chat, empty, with a note; the match and the chat row stay.

| Control | ID | Notes |
| --- | --- | --- |
| Cancel | `chat-clear-cancel` | Left, focused |
| Clear chat | `chat-clear-confirm` | Right |
| Cleared note | `chat-cleared-note` | `chat-cleared` only |

`delete-dialog` confirms deleting an own message for both; deletion is best effort.

| Control | ID | Notes |
| --- | --- | --- |
| Cancel | `delete-cancel` | Left, focused |
| Delete | `delete-confirm` | Right |

## Settings and removal

`settings`. Privacy settings are local to the identity and never published. Defaults: no profile
views and no read receipts sent, typing shown, presence to matches, notifications on. Turning
sending off never stops receiving.

| Control | ID | Notes |
| --- | --- | --- |
| Identities | `settings-identities` | Avatar, short Peer ID, Active or storage kind |
| Remove | `settings-remove`, `settings-remove-2` | Per identity row; the client has it in the active row only, removing another identity is planned |
| Add identity | `settings-create` | Under the no-backup line |
| Blocked people | `blocked` | Avatar and short Peer ID per row |
| Unblock | `unblock-1` | Per blocked row |
| Theme | `theme-system`, `theme-light`, `theme-dark` | System by default |
| Language | `language` | |
| Send profile views | `send-views` | Off |
| Send read receipts | `send-read-receipts` | Off |
| Show typing | `send-typing` | On |
| Active recently to | `presence-matches`, `presence-nobody` | Matches by default |
| New messages and likes | `message-notifications` | On |
| Run a node | `node-enabled` | Off |

`remove-dialog` names the identity by avatar, short Peer ID and kind, then the full ID, withdraws
the profile with a tombstone and deletes the key with this identity's chats, matches and likes on
this computer. `remove-offline`:
no node accepted the tombstone, so the dialog says the profile expires by itself within the
nodes' profile lifetime and offers Retry or Remove anyway.

| Control | ID | Notes |
| --- | --- | --- |
| Peer ID | `remove-peer` | Full ID |
| I understand this key cannot be recovered | `remove-confirm` | Required |
| Cancel | `remove-cancel` | Left, focused; `remove-cancel-offline` when offline |
| Remove identity | `remove-action` | Right |
| Retry | `remove-retry` | Offline only |
| Remove anyway | `remove-anyway` | Offline only |
