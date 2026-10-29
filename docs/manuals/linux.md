# Linux desktop client

The `dyapp` desktop client for Linux is a WebKitGTK window. It opens local identities,
connects to bootstrap nodes, edits and publishes a public profile and searches published
profiles. Chats in the window are planned.

## First launch

With no identity yet, the window explains what an identity is and offers the key storage: system
credential storage, preselected and recommended when the session has it (otherwise the reason is
shown and the app-data file is preselected), or an app-data file with its copy warning. Nothing is
created until **Create identity**. Restoring an existing identity is planned.

## Window layout

An open identity shows a sidebar with the identity on top and four areas: **Profile**, **Search**,
**Network** and **Settings**. Below 1008 px wide the sidebar shrinks to icons (hover one for its
name); below 641 px it hides behind the **Menu** button in the top bar and closes on Escape or a
click outside it.

## Settings

The **Settings** area lists local identities with their key storage kind and marks
the active one. The list and the picker show a short Peer ID like a git hash: "@" and the last 6
characters, longer when another local identity ends the same way; hover a row for the full ID. **Add a new identity** opens the explicit key-storage choice with its backup
warning; **Switch or add an identity** opens the existing picker. On both, **Cancel** returns to
the open identity. These identity actions live only in Settings, and on the recovery screen for a
key that cannot be opened. **Remove this identity…**, in the active identity's row, shows
the selected Peer ID, storage mode and backup warning, then requires a separate checkbox before
Remove is enabled. Cancel closes the dialog and leaves Settings open without deleting anything.

The **Network** area keeps its edits and Check results while you visit other areas. An
unsuccessful Check leaves the prior active network unchanged.

## Connect to a network

After the identity opens, enter the bootstrap nodes you were given, one `host:port` per line
(IPv6 in brackets, optionally `/<peer ID>` to pin the node), and the network name, blank for the
public network. **Check connection** saves the rows and dials each one; every row shows its own
result, and **Continue** turns on only when at least one node of that network answered. Continue
makes these the active settings. The rows and settings belong to the open identity and stay in its
folder; checking never changes the identity key.

## Public profile

Choose any of the nine profile values, or leave every value at **Not published**. Birth year,
month and day are each optional. **Save draft** keeps the values for this identity across
restarts. **Preview public profile** lists the selected fields and exact birth-date parts,
shows the possible age range (or **Age unknown**), and warns that others may copy published
data. A full date has an additional disclosure warning.

To publish, review the current preview and acknowledge that public data may remain available
after deletion. Separately confirm you are 18 or older for this attempt, then choose
**Publish profile**. Editing the form or completing an attempt clears both confirmations.
The app saves the current form, solves the profile puzzle and publishes to an active node;
it checks the signed readback before reporting the version, then sends the profile to up to two
search nodes so others can find it. The app sends a new version to search at most every ten minutes:
until then the profile page says in how many minutes the changes appear in search, and the app
sends them when that time ends or on the next start. You can stop puzzle work without
losing the draft or its progress. A network failure keeps the draft for retry; when no node of the
active network answered, the error shows **Open Network**, which opens the Network area. A supplied
birth date must be valid and, when it includes a year, must not allow an age under 18.

## Find people

**Search** asks up to three search nodes of the active network for a sample of published
profiles. A sample is not a complete list, and profile fields are what each person states about
themselves. The app checks every profile's signature and filters before showing it and drops the
rest. After the first sample, filters appear for the fields every reached node offers: an age
range (18 or older), a two-letter country code and the profile values. The filters you set stay
in the form. Each result shows the Peer ID; open it to see the public fields.

While a search runs, its buttons are disabled; the rest of the window stays usable. An empty
sample means it contained no verified matches, not that none exist. When some nodes fail, the
matches found are kept, each node problem is listed (no node, unreachable, timeout, busy with the
seconds to wait, an unsolved puzzle or a forged reply) and **Retry** asks again. When no node of
the active network answered, the error shows **Open Network** instead of Retry. **Load another
sample** runs the same search again.

## Run a node

While the window is open, the app can run a store and search node for the open identity on its
active network ([ADR 0030](../decisions/0030-desktop-full-node.md)). It is off by default. When
on, it uses up to 1 GiB of disk by default (a quarter for profiles, half for mailboxes, a quarter
for the search index) and 64 connections, and keeps its own node key and stores in the
identity's folder. Switching identities stops the previous identity's node; removing an
identity removes its node files too.

Turn it on in **Settings → Help the network**: tick the box, set the disk limit in MB (at least 4)
and the connection limit, then **Save node settings**. The status line shows the node ID while it
runs, says it waits for a network when none is active yet, or shows why it did not start.

## Build

```sh
make prepare  # once: the Docker images
make deb      # → build/deb/dyapp_<version>_<arch>.deb and dyapp-<version>-<machine>-linux-gnu.tar.gz
```

Both are built in Debian 12, so they run on Debian 12 and newer systems with glibc 2.36 or later.
`make deb` installs each on a clean Debian 12 and opens and closes the window under Xvfb.

Published release downloads are planned.

## Install the package

```sh
sudo apt install ./dyapp_<version>_<arch>.deb
```

`apt` pulls the dependencies: WebKitGTK 4.1 (`libwebkit2gtk-4.1-0`) and GTK 3. The package installs
`/usr/bin/dyapp`, a menu entry (`/usr/share/applications/dyapp.desktop`), an icon, and
AppStream metadata (`/usr/share/metainfo/io.github.abbat.dyapp.metainfo.xml`). The launcher and
software-center summary have Russian translations; the package's Debian `Description` remains
English. Remove it with `sudo apt remove dyapp`.

## Install from the tarball

The tarball holds `dyapp`, `dyapp.desktop`, `dyapp.png` and `LICENSE`. Install the libraries first
(Debian and Ubuntu: `sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0`), then:

```sh
tar -xzf dyapp-<version>-<machine>-linux-gnu.tar.gz
./dyapp-<version>-<machine>-linux-gnu/dyapp
```

For a menu entry, copy `dyapp` to `~/.local/bin/`, `dyapp.desktop` to
`~/.local/share/applications/` and `dyapp.png` to `~/.local/share/icons/hicolor/32x32/apps/`.
