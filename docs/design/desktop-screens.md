# Desktop screens

Control IDs and states for every screen of the Linux and Windows shell in
[ADR 0033](../decisions/0033-desktop-navigation-shell.md). The implementation and its UI tests use
these IDs; IDs already in the current client keep their meaning. The static prototype in
`clients/desktop/prototype/` is the layout reference: it shows every state with the shipped
stylesheet `clients/desktop/ui/style.css`, is not shipped itself and has no logic. `make ui-prototype` saves every state in English and
Russian at 440 and 1280 px to `build/ux-research/prototype/<state>-<lang>-<width>.png`
([UI testing](../testing/ui-testing.md#desktop-prototype-screenshots)). The client already has the
shell with the Profile, Search, Chats, Network and Settings areas and both first-run steps.
Chats now shows local requests, conversations and text history with send, Edit, Delete and Clear
controls; remaining chat actions and the switcher are planned. Its controller is
`clients/desktop/ui/chats-ui.js`, loaded after `search.js` because it uses Search's profile
cards and place lookup; `chats-photo-send.js` follows it and sends a picked photo, and
`profile-photos.js` follows that and shares its canvas encoding for the profile editor.
[v0.0.1 client screens](v0.0.1-client-screens.md) holds the behaviour
contracts.

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
switcher, Settings) leads each row with a round avatar: for the open identity the first photo of
its profile draft, read locally without contacting a node, otherwise the two characters after "@"
in the short Peer ID, because other identities' stores stay closed. A short Peer ID is "@" and the
last 6 characters, longer until unique among the identity's known peers; the
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

`profile`: the draft with **Hide from search**, whose hint says a known Peer ID still opens the
profile and fetched copies remain. `profile-hidden`: the switch is on, a tombstone is
published and a notice explains how to return. `profile-dark`: the same screen in the dark theme.

| Control | ID | Notes |
| --- | --- | --- |
| Status | `profile-status` | Draft, published with version, or hidden |
| Hide from search | `profile-hidden` | Switch |
| Hidden notice | `profile-hidden-notice` | Only while hidden |
| Photos | `profile-photos` | Up to six tiles, the first marked main; Move left, Move right and Remove on each, with no move past an end |
| Add photo | `profile-photo-add` | Disabled at six; `profile-photo-limit` then replaces `profile-photo-hint` |
| Public name | `profile-name` | Optional, at most 64 characters, no hidden symbols |
| Gender | `profile-gender` | |
| Looking for | `profile-goal` | |
| Birth year, month, day | `birth-year`, `birth-month`, `birth-day` | Month and day optional |
| Shown age | `profile-age` | "31 or 32" for a partial date |
| Country, town | `profile-country`, `profile-place` | |
| More about you | `profile-more` | Collapsed optional fields |
| Save draft | `profile-save` | |
| Preview and publish | `profile-publish` | Primary; opens the dialog |

`photo-remove-dialog` asks before a photo leaves the draft: `photo-remove-cancel` is left and
focused, `photo-remove-confirm` is right. A change keeps focus on the moved photo, or moves it to
Add photo after an add or a removal. Status line states are in
[profile photos](profile-photos.md#states).

`publish-dialog` lists exactly the public fields, with the number of photos, and needs both boxes;
`publish-progress` shows the proof of work with elapsed time. Cancel is left and focused.

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
Block hides the peer from local search and chats, sits apart from Like as a quiet button under
the fields, and is undone in Settings. It also removes the peer from the Liked you list. A like
that fails to send but stays stored says it goes out later; the catch-up pass retries it. Pass, a
quiet button beside Block, hides the peer from this device's later search and Liked you lists and
sends no signal. It replaces Like, is not offered while a like stands, and is undone in the same
place (`view-unpass`) or in Settings; chats and mail are not affected. The
desktop command persists the local block before trying to revoke issued write tokens through one
node. Search reports whether revocation was unneeded, still pending, or confirmed by that node.
Settings offers Retry token revocation for each blocked peer. After reopening, the network
status is unknown because the acknowledgement is not stored; Retry is available. A successful
node reply is best effort, not network-wide removal.

| Control | ID | Notes |
| --- | --- | --- |
| Back to results | `view-back` | |
| Photos | `view-photos` | Gallery with thumbnails under it; on a metered or unknown network `view-photo-load` loads the full photo |
| Peer ID | `view-peer` | Short ID |
| Copy | `copy-view-peer` | Copies the full ID |
| Compare safety number | `view-safety` | Opens `view-safety-panel` with grouped `view-safety-number` from both Peer IDs |
| Verified notice | `view-verified` | Shown while the mark holds for the cached profile's devices |
| Mark verified | `view-confirm` | In the panel; opens `verify-dialog` |
| Remove verified mark | `view-unverify` | Replaces `view-confirm`; takes the mark off at once |
| Copy safety number | `copy-safety-number` | Copies grouped digits; attempts to clear after 60 seconds only while unchanged |
| Match notice | `view-match` | Match only |
| Message | `view-message` | `view-message-match` after a match |
| Like | `view-like` | `view-unlike` (Liked) after a match |
| Fields | `view-fields` | Looking for, children, interests, languages |
| Claims note | `view-claims` | Written by the owner; nobody checks it |
| Pass | `view-pass` | Quiet, beside Block; `view-unpass` after a pass |
| Block | `view-block` | Quiet, under the fields |

`profile-view-safety` is the expanded comparison panel. `profile-view-verified` is the same panel after a
confirmation, and `verify-dialog` is the question before it. The number and rendered QR are derived
from the active identity and the verified contact Peer ID. The QR carries a version byte followed
by both raw Peer IDs in shower/scanner order. Comparing digits, displaying the QR,
importing an image or scanning with the camera does not record a verified
contact. Both scan paths reject foreign codes with a localized result; image
import also rejects malformed and multiple codes. The camera stops on exit and
reports denial or an unavailable device. A real Debian 12 camera check is
pending ([capture notes](desktop-camera-capture.md)).
Only `view-confirm` followed by `verify-confirm` in `verify-dialog` sets the mark; Cancel is
focused and sends nothing. The panel shows progress and a reason for every failure in
`view-safety-scan-status`, and a failure leaves the mark as it was. On success the view shows the
returned fresh profile. An answer for an earlier identity or another open profile changes
nothing on screen. `view-verified` reads the mark through `contact_verified` for the profile
cached in this session, so a card with no cached profile shows no mark, and a cached profile that
lists a new device clears it and raises `chat-new-device`. `unverify_contact` removes the mark with no network and no dialog,
because it only lowers trust and Mark verified restores it.
The `confirm_contact` command is the only writer of the mark: it fetches the signed profile from
every reachable seed, keeps the newest observed version and refuses the identity's own Peer ID,
another owner's profile, a version below one already seen in this identity and network context, a
tombstone, and a run with no answering seed. That check is held in memory, so a restart forgets
it.
The copy button does not change verification state. Its native clipboard hint
and timed cleanup are best effort
([privacy limits](../security/privacy.md#safety-number-clipboard)).

`unlike-dialog` confirms taking the like back.

| Control | ID | Notes |
| --- | --- | --- |
| Cancel | `unlike-cancel` | Left, focused |
| Take back | `unlike-confirm` | Right |

## Chats

`chats`: the active identity's chats only; there is no shared inbox. **Liked you** lists
received likes; liking back makes a match. **Message requests** come from non-matches; the sender
learns nothing until a reply. The Linux screen now renders ordered, pair-wide local requests
and conversations with a short Peer ID, latest text or deletion placeholder, original message
time when present, and local unread count. Request rows carry an explicit message-request marker
until a reply or match opens the conversation. In history, HTTP(S) text remains inert while a
caption shows each parsed full host, with IDNs in punycode and no remote preview. It shows local
rows after mailbox refresh even if that refresh or the network-backed **Liked you** lookup fails.
Opening a row shows its current message history and marks incoming messages read on this device; Back returns to the list.
The Linux detail now lets a received request reply and an open conversation send text.
A sent request shows a disabled composer until the peer replies or a match opens the chat.
A first message to a peer with no KeyPackage on the node keeps the draft and says to try later.
A send error keeps an unstaged draft; if the text was already staged locally, the UI shows it
in history and reports pending delivery. A verified Found profile's Message action opens an
empty first-message detail; after one send it becomes a sent request and disables the composer.
A received photo shows its local preview over a caption with its type, size and, in the last day
of the 168-hour node retention, the time left of the original, unless this computer keeps it. Without a preview, or with one that
cannot be decoded, the caption stands alone. Before the conversation is accepted, a preview stays
blurred until Show photo. Photos offer no edit, delete or forward. Download original fetches it on a tap and shows it at
up to 480 pixels. An opened original stays sealed on this computer up to the limit in Settings →
Photos (do not keep, 256 MiB by default, or 1 GiB), the longest unopened leaving first, and such a
photo opens by itself in any download mode, past the retention too. A lower limit asks first, then
erases what no longer fits at once; Clear chat erases the chat's originals.
A failed fetch offers a retry; an original no node holds shows "No longer available", and one that
fails its checks "This photo cannot be shown". In an open chat with an accepted person, originals
also download by themselves under Settings → Photos: unless the network is metered or unknown (the
default), always, or only on request. A switch to an unmetered network picks up what waited at the
next mail pass. The composer sends one photo at a time. [Profile photos](profile-photos.md) remain
planned for this UI. An own, live text can be edited; the dialog
starts with its current text, keeps an unstaged draft on failure, and shows a pending delivery
status when the edit was staged locally. Delete is available on live own text only; confirmation
warns that the peer, nodes, other devices and backups may keep copies. The local history shows a
terminal placeholder after deletion, including when network delivery remains pending. Own
messages show a persisted pending, accepted-by-node or received-by-one-device label; node
acceptance does not imply peer delivery, and none of these labels claims the message was read.
A live own text covered by the peer's read receipt shows Read instead. The label appears only if
the peer turned receipts on, so its absence says nothing; see
[read receipts](read-receipts.md). Opening a chat counts as reading only while the window is
visible.
Received forwarded text has a Forwarded label in history and the Chats list preview; the author
shown is the verified MLS sender. The desktop forward command validates a live source text and
an open target conversation before sending. Forward opens a picker of other open chats, allows
Cancel without staging, and shows a queued status if the network fails after local staging. The
open chat refreshes after the minute catch-up pass, including receipt-only changes. Clear chat asks
for confirmation,
deletes this computer's history, keeps the conversation row and shows a local-only note. Block
is available from a request or open chat: it hides the peer locally at once and reports whether
network token revocation is pending. The Rust desktop commands expose history, send text, Clear
chat and mark read for the active identity.

| Control | ID | Notes |
| --- | --- | --- |
| Liked you | `chats-liked` | With count |
| Message requests | `chats-requests` | With count |
| Conversations | `chat-list` | Rows `chat-1`, `chat-2`; a row leads with the peer's first thumbnail while their profile is in memory, otherwise the letters, and shows "Active recently" for a match |

`chat`: one conversation with a match. The prototype's peer avatar and profile link remain planned.
A peer is shown by the local name if it has one, else by its public name with "Verified" or "Not
verified" and the short Peer ID, else by the short Peer ID; the full ID stays in the title. This
holds in the list, the header, messages, the forward list, search cards and the profile view. A
public name needs the peer's
profile in the in-memory cache, so after a restart it returns with the next search or chat fetch.
"Active recently" is shown under the peer, and no online or last-seen time is shown. The current Linux message
status is pending, accepted by a node, received by one peer device, or read when the peer sent an
opt-in read receipt. An edited message says so, a deleted one leaves a
placeholder, and Edit and Delete apply to live own messages with best-effort delivery.
`chat-first`: to a non-match, one message is sent and the composer stays disabled
until a reply or a match. `chat-request`: an incoming message from a non-match, with Reply through
the composer or Block. `chat-new-device`: a profile no older than the newest one seen lists a device
that was not there, whether or not the contact was ever verified. The first profile seen of a
contact only records its devices. The store clears any mark and raises a per-contact flag in one
statement, so the notice survives a restart, and only `chat-new-device-ack` or a new verification
ends it; the chat stays usable meanwhile. Acknowledging does not restore the mark. A legitimate
link and a stolen key look the same, so a link does raise the notice; the devices count as seen
when it is raised, so several links before the user looks show one notice and a read of the same
profile raises none. A device that a newer profile removed is new again when it returns. The page
reads
the flag through `contact_new_device` when a chat opens and on each mail pass; a failed read keeps
what is shown. An older profile changes nothing, and an answer for another chat or an earlier
identity shows nothing. The check runs only once a fresh profile reached this session's cache from search, a send or a mail pass.
"Anna is typing…" shows for 10 seconds after the last typing signal, only
over a direct path.

| Control | ID | Notes |
| --- | --- | --- |
| Back | `chat-back` | |
| Peer | `chat-profile` | Opens the found profile |
| Presence | `chat-presence` | "Active recently" |
| More | `chat-more` | Opens `chat-menu` |
| Name this contact | `chat-rename` | In the menu, opens `chat-rename-dialog` |
| Clear chat | `chat-clear` | In the menu |
| Block | `chat-block` | In the menu |
| First-message notice | `chat-first-notice` | `chat-first` only |
| New-device notice | `chat-new-device-notice` | `chat-new-device` only; stays until acknowledged |
| I understand | `chat-new-device-ack` | Ends the notice, keeps the mark off |
| Request notice | `chat-request-notice` | `chat-request` only |
| Block | `request-block` | In the request notice |
| Message actions | `msg-menu` | Edit and Delete on own messages |
| Delete | `msg-delete` | Opens `delete-dialog` |
| Typing | `chat-typing` | |
| Message | `chat-input` | |
| Attach photo | `chat-attach` | Open conversations only; picks a JPEG or PNG and sends it at once. It goes without its metadata, at most 2560 px, and a refusal stays in the composer status |
| Send | `chat-send` | Disabled while the box is empty |

`chat-clear-dialog` confirms Clear chat: the messages are deleted from this computer only, the
peer keeps theirs, cleared messages do not come back and unsent ones are cancelled
([contract](message-edits-and-deletion.md#clearing-a-conversation)). `chat-cleared`: the
same chat, empty, with a note; the match and the chat row stay.

| Control | ID | Notes |
| --- | --- | --- |
| Cancel | `chat-clear-cancel` | Left, focused |
| Clear chat | `chat-clear-confirm` | Right |
| Cleared note | `chat-cleared-note` | `chat-cleared` only |

`chat-rename-dialog` sets a name only this identity sees; it is stored locally and never published
or sent. It replaces the public name everywhere, and an empty field removes it. A name of more than
64 characters or with hidden symbols is refused with an explanation.

| Control | ID | Notes |
| --- | --- | --- |
| Name | `chat-rename-input` | Starts from the local name |
| Cancel | `chat-rename-cancel` | Left |
| Save name | `chat-rename-save` | Right |

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
| Retry token revocation | Implemented in desktop UI | Per blocked row; status is announced below the list |
| Unblock | `unblock-1` | Per blocked row |
| Passed people | `passed` | Avatar and short Peer ID per row |
| Undo pass | `unpass-1` | Per passed row |
| Theme | `theme-system`, `theme-light`, `theme-dark` | System by default |
| Language | `language` | |
| Send profile views | `send-views` | Off |
| Send read receipts | `send-read-receipts` | Off |
| Show typing | `send-typing` | On |
| Active recently to | `presence-matches`, `presence-nobody` | Matches by default |
| New messages and likes | `message-notifications` | On: a notice when mail arrives and the window has no focus; a click shows the window. Linux only so far |
| What a notice shows | `notice-content` | "New message" only; or the sender's name; or the name and text. Disabled while notices are off. Requests and likes stay fixed |
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
