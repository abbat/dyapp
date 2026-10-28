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
Peer ID: 12D3…Fk9Qx (peer-id) | [Copy (copy-peer-id)]
Bootstrap addresses (seeds)
{+ host:port per line }
[▸ Private network] network name (network-name)
[Skip for now (skip-network)] | [Check and connect (check-connection)]
}
@endsalt
```

## Picker and recovery

`picker`: several identities and none active, or after a removal. `recovery`: the active identity
cannot be opened; the localized reason, then Details with the state code.

```plantuml
@startsalt
{+
<b>Choose an identity
[12D3…Fk9Qx · System keyring (pick-1)]
[12D3…7aZ2p · Key file (pick-2)]
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
[Fk 12D3…Fk9Qx · System keyring (identity-switcher)] | Active section
{+ 12D3…7aZ2p (switch-to-2) | Add identity (add-identity-menu) } (identity-menu) | .
[Profile (nav-profile)] | .
[Search (nav-search)] | .
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

`network`, `network-checking` and `network-error`. One **Check and connect**; a failure or cancel
keeps the prior network. The error shows Details with the code and the row outcomes.

```plantuml
@startsalt
{+
<b>Network
Status (network-status)
Error notice · [▸ Details] (network-error-details) [Copy (copy-network-details)]
Bootstrap addresses (network-seeds)
[▸ Private network] (network-name-field)
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

## Settings and removal

```plantuml
@startsalt
{+
<b>Settings
Identities (settings-identities)
12D3…Fk9Qx · Active | [Remove (settings-remove)]
12D3…7aZ2p · Key file | [Remove (settings-remove-2)]
No key backup in this version
[Add identity (settings-create)]
Theme (X) System (theme-system) () Light (theme-light) () Dark (theme-dark)
Language ^English^ (language)
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
12D3KooWQx…Fk9Qx (remove-peer)
[ ] I understand this key cannot be recovered (remove-confirm)
[Cancel (remove-cancel)] | [Remove identity (remove-action)]
offline: [Cancel (remove-cancel-offline)] | [Retry (remove-retry)] | [Remove anyway (remove-anyway)]
}
@endsalt
```
