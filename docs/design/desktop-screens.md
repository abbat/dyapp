# Desktop screens (planned)

PlantUML Salt cards for every screen and state of the Linux and Windows shell in
[ADR 0033](../decisions/0033-desktop-navigation-shell.md). Each card lists the control IDs the
implementation and its UI tests must use; IDs already in the current client keep their meaning.
The static prototype in `clients/desktop/prototype/` shows the same screens with the proposed
stylesheet; it is not shipped and has no logic. `make ui-prototype` saves every screen in English
and Russian at 440 and 1280 px to `build/ux-research/prototype/<screen>-<lang>-<width>.png`
([UI testing](../testing/ui-testing.md#desktop-prototype-screenshots)). The current one-page UI
stays until the redesign lands; [v0.0.1 client screens](v0.0.1-client-screens.md) holds the
behaviour contracts.

The prototype wording is a draft; the shipped strings stay in `clients/desktop/ui/i18n`.

## First run

`wizard-identity`: step 1 of 2. The system keyring is preselected when available; the file option
says the key is kept unencrypted. Nothing is created before **Create identity**.

```plantuml
@startsalt
{+
Step 1 of 2
<b>Create your identity
(X) System keyring (system-choice)
() File in the app folder, unencrypted (file-choice)
No key backup in this version (no-backup)
[Create identity (create-identity)]
}
@endsalt
```

`wizard-network`: step 2 of 2, may be skipped. Same form and outcome as the Network screen.

```plantuml
@startsalt
{+
Step 2 of 2
Peer ID: 12D3KooWQxMASi…Fk9Qx2, full (peer-id) | [Copy (copy-peer-id)]
Network name "Public network" (network-name)
Bootstrap addresses (seeds)
{+ host:port per line }
[Skip for now (skip-network)] | [Check and connect (check-connection)]
}
@endsalt
```

## Picker and recovery

`picker`: several identities and none active, or after a removal. Every identity list (picker,
switcher, Settings) leads each row with a round avatar: the thumbnail of the identity's first
profile photo, or the first two characters of the short Peer ID when it has none. A short Peer
ID is "…" and the last 6 characters, longer until unique among the identity's known peers; the
full ID shows in the wizard, recovery and removal. `recovery`: the active identity
cannot be opened; the localized reason, then Details with the state code.

```plantuml
@startsalt
{+
<b>Choose an identity
[(photo) …Fk9Qx2 · System keyring (pick-1)]
[(7a) …7aZ2pL · Key file (pick-2)]
[Add identity (add-identity)]
}
@endsalt
```

```plantuml
@startsalt
{+
<b>This identity cannot be opened
Localized reason (recovery-reason)
[▸ Details] code and row (recovery-details) [Copy (copy-recovery-details)]
Peer ID (recovery-peer)
[Remove (remove-identity)] | [Use another identity (switch-identity)] | [Retry (retry)]
}
@endsalt
```

## Shell

Every later screen sits in the shell. `switcher` opens the identity menu; `menu` is the
off-canvas sidebar at 640 px and below, opened by `menu-open` in the top bar.

```plantuml
@startsalt
{#
[(photo) …Fk9Qx2 · System keyring (identity-switcher)] | Active section
{+ …7aZ2pL (switch-to-2) | Add identity (add-identity-menu) } (identity-menu) | .
[Profile (nav-profile)] | .
[Search (nav-search)] | .
[Chats · 3 (nav-chats, chats-unread)] | .
[Network (nav-network)] | .
[Settings (nav-settings)] | .
● Connected · Public network (connection-status) | .
}
@endsalt
```

`connection-status` shows Connecting, Connected (one compatible node answered) or Offline and
opens Network.

## Profile

`profile`: the draft with **Hide from search**. `profile-hidden`: the switch is on, a tombstone is
published and a notice explains how to return. `profile-dark`: the same screen in the dark theme.

```plantuml
@startsalt
{+
<b>Profile
Published, version 3 (profile-status)
Hide from search | [ ] (profile-hidden)
Hidden notice (profile-hidden-notice)
Gender ^Woman^ (profile-gender) | Looking for ^…^ (profile-goal)
Year "1994" (birth-year) | Month ^^ (birth-month) | Day ^^ (birth-day)
Shown as 31 or 32 (profile-age)
Country ^Germany^ (profile-country) | Town "Leipzig" (profile-place)
[▸ More about you] (profile-more)
[Save draft (profile-save)] | [Preview and publish (profile-publish)]
}
@endsalt
```

`publish-dialog` lists exactly the public fields and needs both boxes; `publish-progress` shows the
proof of work with elapsed time. Cancel is left and focused.

```plantuml
@startsalt
{+
<b>Publish this profile?
Public fields (profile-public-fields)
[ ] I am 18 or older (profile-attested)
[ ] Public fields may be copied (profile-disclosure)
Progress bar · 0:24 elapsed (publish-progress only)
[Cancel (profile-cancel)] | [Publish (profile-publish-confirm)]
}
@endsalt
```

## Network

`network`, `network-checking` and `network-error`. The network name stands above the addresses,
its placeholder says "Public network" when empty. One **Check and connect**; a failure or cancel
keeps the prior network. The error shows Details with the code and the row outcomes.

```plantuml
@startsalt
{+
<b>Network
Status (network-status)
Error notice · [▸ Details] (network-error-details) [Copy (copy-network-details)]
Network name "Public network" (network-name-field)
Bootstrap addresses (network-seeds)
node1.example.org:33445 | Answered / Checking / No answer (seed-rows)
[Cancel (cancel-check)] | [Check and connect (check-and-connect)]
}
@endsalt
```

## Search

`search`, `search-loading`, `search-empty` and `search-error`.

```plantuml
@startsalt
{+
<b>Search
Age from "25" (search-age-min) | to "35" (search-age-max)
Country (search-country) | Town (search-place)
[▸ More filters] (search-more)
[Search (run-search)]
2 profiles (search-status)
Result cards (search-results)
[Show another sample (search-again)]
Loading skeleton | Empty: [Show another sample (search-again-empty)]
Error · [▸ Details] (search-failures) [Copy (copy-search-details)] [Retry (search-retry)]
}
@endsalt
```

## Found profile

`profile-view` opens from a result card (`result-1`, `result-2`) inside Search, which stays
selected. Search lists load only thumbnails; the full photos load here. Field values are claims
of the owner and say so (`view-claims`); nothing is shown as verified. Before a match the hint
says one message can be sent until a reply. `profile-view-match`: a mutual like adds the match
notice, Like becomes Liked (`aria-pressed`), and pressing it sends the unlike, which ends the
match for both. Block is local and hides the peer from search and chats.

```plantuml
@startsalt
{+
[< Back to results (view-back)]
Photo gallery, thumbnails under it (view-photos)
<b>Anna
Woman, 29–30 · Leipzig, Germany
Peer ID …Ns4Lq8 (view-peer) | [Copy (copy-view-peer)]
match: You like each other (view-match)
[Block (view-block)] | [Message (view-message)] | [♥ Like (view-like)]
match: [Block] | [♥ Liked (view-unlike)] | [Message (view-message-match)]
Looking for, children, interests, languages (view-fields)
Written by the profile owner; nobody checks it (view-claims)
}
@endsalt
```

## Chats

`chats`: the active identity's chats only; there is no shared inbox. **Liked you** lists
received likes; liking back makes a match. **Message requests** come from non-matches; the sender
learns nothing until a reply (read receipts are off by default). Conversations and requests show the
peer's photo thumbnail (letters only without a photo), the public name or short Peer ID, the last message, its time and the unread count.

```plantuml
@startsalt
{+
<b>Chats
[(avatars) Liked you · 2 (chats-liked)]
[(photo) Message requests · 1 (chats-requests)]
Conversations (chat-list)
[(photo) Anna · See you on Saturday then! · 14:32 · 2 (chat-1)]
[(photo) …b8Wte3 · You: Hi! … · Yesterday (chat-2)]
}
@endsalt
```

`chat`: one conversation with a match. The peer's avatar and name in the header
(`chat-profile`) open the found profile, so a narrow window keeps one text button. The header shows "Active recently" when the last signal
from the peer is at most 2 hours old (`chat-presence`); there is no online or last-seen time.
Own messages show Sent or Delivered, and Read only when the peer sends read receipts; an edited
message says so, and a deleted one leaves a placeholder. Edit and Delete (`msg-menu`) apply to
own messages and are best effort. `chat-first`: to a non-match, one message is sent and the
composer stays disabled until a reply or a match. `chat-request`: an incoming message from a
non-match, with Reply through the composer or Block. "Anna is typing…" (`chat-typing`) shows for 10 seconds after
the last typing signal, only over a direct path.

```plantuml
@startsalt
{+
[< (chat-back)] | [(photo) Anna · Active recently (chat-profile, chat-presence)] | [Block (chat-block)]
first: notice (chat-first-notice) · request: notice (chat-request-notice)
Today
                       Hi Anna! … 14:05
Thanks! … 14:11
<i>Message deleted
                Shall we …? edited · 14:20 · Delivered [Edit] [Delete] (msg-menu)
"Message" (chat-input) | [Send (chat-send)]
}
@endsalt
```

## Settings and removal

Privacy settings are local to the identity and never published. Defaults: no profile views and
no read receipts sent, typing shown, presence to matches, notifications on. Turning sending off
never stops receiving.

```plantuml
@startsalt
{+
<b>Settings
Identities (settings-identities)
(photo) …Fk9Qx2 · Active | [Remove (settings-remove)]
(7a) …7aZ2pL · Key file | [Remove (settings-remove-2)]
No key backup in this version
[Add identity (settings-create)]
Theme (X) System (theme-system) () Light (theme-light) () Dark (theme-dark)
Language ^English^ (language)
Privacy: [ ] views (send-views) [ ] read receipts (send-read-receipts) [X] typing (send-typing)
Active recently: (X) Matches (presence-matches) () Nobody (presence-nobody)
[X] New messages and likes (message-notifications)
[ ] Run a node (node-enabled)
}
@endsalt
```

`remove-dialog` withdraws the profile with a tombstone, then deletes the key. `remove-offline`:
no node accepted the tombstone, so the dialog says the profile expires by itself within the
nodes' profile lifetime and offers Retry or Remove anyway.

```plantuml
@startsalt
{+
<b>Remove this identity?
12D3KooWQxMASi…Fk9Qx2, full (remove-peer)
[ ] I understand this key cannot be recovered (remove-confirm)
[Cancel (remove-cancel)] | [Remove identity (remove-action)]
offline: [Cancel (remove-cancel-offline)] | [Retry (remove-retry)] | [Remove anyway (remove-anyway)]
}
@endsalt
```
