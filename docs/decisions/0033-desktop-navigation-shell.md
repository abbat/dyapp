# 0033. Desktop navigation shell

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

The Linux and Windows client (`clients/desktop/ui`) shows every feature on one centred page:
same-looking full-width buttons, about 30 lines of CSS and no navigation. The maintainer judged it
unusable. [Desktop UX references](../design/desktop-ux-references.md) collected how GNOME,
Fluent, Apple and Material guidelines and key-as-account apps (Signal, Session, Element, Jami,
Syncthing, KeePassXC) solve the same tasks, and listed the open choices in its section 9. The
desktop app has four areas (Profile, Search, Network, Settings), several local identities, no key
backup in v0.0.1, and the Android client must keep the same control IDs and decision order.

## Decision

The desktop client gets one app shell:

- **First run** is a short full-window wizard before the shell exists: create an identity, then
  the network form, which may be skipped. The shell opens on Profile.
- **Navigation** is a left sidebar with icon and label for Profile, Search and Network, and
  Settings pinned last. It becomes an icon rail at 641-1007 px and an off-canvas menu at 640 px
  and below. The layout works down to a 400 px wide window.
- **Identity switcher** at the top of the sidebar shows the active identity by its shortened Peer
  ID and storage kind, and lists the others plus "Add identity". Removal lives in Settings.
- **Connection status** at the bottom of the sidebar shows connected or not and opens Network.
- **Network form** shows the bootstrap addresses at once, the network name under a disclosure,
  and one **Check and connect** action with Cancel; a failure or cancel keeps the prior network.
- **Confirmations** are modal dialogs, Cancel left and the action right, focus on Cancel. Removing
  an identity, the active one included, needs a ticked "I understand this key cannot be recovered"
  box; afterwards the picker opens, or the wizard when no identity remains. Publishing opens a
  dialog with the profile preview and the consent boxes.
- **Backup:** no export or restore in v0.0.1; a calm "no backup" line at creation and in Settings,
  with no repeating reminder.
- **Styling** is one own stylesheet with CSS custom properties (spacing, type roles, colours) on
  system fonts, no framework. Theme follows the system by default, with System, Light and Dark in
  Settings.

## Consequences

- Returning users reach every area in one click, and the wizard keeps first run linear.
- The shell, collapse states, focus handling in dialogs and the off-canvas menu are built by hand
  and need UI tests at 440 and 1280 px.
- A ticked box is weaker than typing a Peer ID suffix; revisit if users remove keys by mistake.
- The theme choice adds a setting and a test; WebKitGTK must honour the forced scheme.
- Restore, key export, local identity labels and a macOS sidebar are later work; the wizard and
  the switcher must leave room for a Restore entry.
