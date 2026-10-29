# 0033. Desktop navigation shell

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

The Linux and Windows client (`clients/desktop/ui`) shows every feature on one centred page:
same-looking full-width buttons, about 30 lines of CSS and no navigation. The maintainer judged it
unusable. [Desktop UX references](../design/desktop-ux-references.md) collected how GNOME,
Fluent, Apple and Material guidelines and key-as-account apps (Signal, Session, Element, Jami,
Syncthing, KeePassXC) solve the same tasks, and listed the open choices in its section 9. The
desktop app has five areas (Profile, Search, Chats, Network, Settings), several local identities, no key
backup in v0.0.1, and the Android client must keep the same control IDs and decision order.

## Decision

The desktop client gets one app shell:

- **First run** is a short full-window wizard before the shell exists: create an identity, then
  the network form, which may be skipped. The shell opens on Profile.
- **Navigation** is a left sidebar with icon and label for Profile, Search, Chats (with an unread
  count) and Network, and Settings pinned last. It becomes an icon rail at 641-1007 px and an off-canvas menu at 640 px
  and below. The layout works down to a 400 px wide window.
- **Identity switcher** at the top of the sidebar shows the active identity by a round avatar
  (its first profile photo, else letters of its short Peer ID), shortened Peer ID and storage kind, and lists
  the others plus "Add identity". Removal lives in Settings.
- **Peer ID** is shortened like a git hash: "…" and the last 6 characters, longer until it is
  unique among the identity's known peers. Every Ed25519 Peer ID starts with `12D3KooW`, so the
  start tells nothing apart. The short form only helps to recognize a peer and is no check: a
  key with the same tail can be found. The full ID shows where it is checked or copied: the
  wizard, recovery and removal. Avatar letters are the first two of the short form.
- **Found profile** opens from a search result in the Search area: the photos, the public
  fields marked as unchecked claims, and Like, Message and Block; a mutual like shows a match
  notice and Liked, which takes the like back.
- **Chats** lists, for the active identity only, who liked you, message requests from
  non-matches and the conversations. A conversation shows "Active recently" for a match, edited
  and deleted marks, Sent or Delivered on own messages, and Edit and Delete on them. To a
  non-match one message is sent, then the composer waits for a reply or a match.
- **Connection status** at the bottom of the sidebar shows connected or not and opens Network.
- **Network form** shows the network name ("Public network" when empty) and the bootstrap
  addresses at once, and one **Check and connect** action with Cancel; a failure or cancel keeps the prior network.
- **Confirmations** are modal dialogs, Cancel left and the action right, focus on Cancel. Removing
  an identity, the active one included, needs a ticked "I understand this key cannot be recovered"
  box; afterwards the picker opens, or the wizard when no identity remains. Publishing opens a
  dialog with the profile preview and the consent boxes.
- **Backup:** no export or restore in v0.0.1; a calm "no backup" line at creation and in Settings,
  with no repeating reminder. A file-stored key has no passphrase in v0.0.1; the creation step
  says it is kept unencrypted in a file.
- **Launch** with an active network connects at once and opens Profile; the sidebar status shows
  connecting, connected (one compatible node answered) or offline, which opens Network.
- **Errors** show a localized sentence from the state code, with the untranslated code and the
  affected row under a collapsed "Details" disclosure that has a Copy button.
- **Profile visibility:** a "Hide from search" switch on Profile publishes a tombstone and turns
  back on by publishing the saved draft as a new version. Removing an identity first publishes a
  tombstone in the same action; if no node accepts it, the dialog says the profile disappears by
  itself after the nodes' profile lifetime (30 days by default) and offers Retry or Remove anyway.
- **Publishing progress:** the proof of work takes about a minute on a desktop, so the publish
  dialog shows progress with elapsed time and Cancel.
- **Styling** is one own stylesheet with CSS custom properties (spacing, type roles, colours) on
  system fonts, no framework. Theme follows the system by default, with System, Light and Dark in
  Settings.

## Consequences

- Returning users reach every area in one click, and the wizard keeps first run linear.
- The shell, collapse states, focus handling in dialogs and the off-canvas menu are built by hand
  and need UI tests at 440 and 1280 px.
- A ticked box is weaker than typing a Peer ID suffix; revisit if users remove keys by mistake.
- The theme choice adds a setting and a test; WebKitGTK must honour the forced scheme.
- Restore, key export, a key-file passphrase and local identity labels are later work; the wizard
  and the switcher must leave room for a Restore entry. macOS keeps its own native client and is
  not bound by this shell.
- Sending a tombstone and the hide switch need a client-core call and tests on every platform; a
  hidden profile must not send heartbeats.
