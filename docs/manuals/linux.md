# Linux desktop client

The `dyapp` desktop client for Linux is a WebKitGTK window. This first version is a minimal shell:
it starts, renders and closes. Profiles, search and chats in the window are planned.

## Connect to a network

After the identity opens, enter the bootstrap nodes you were given, one `host:port` per line
(IPv6 in brackets, optionally `/<peer ID>` to pin the node), and the network name, blank for the
public network. **Check connection** saves the rows and dials each one; every row shows its own
result, and **Continue** turns on only when at least one node of that network answered. Continue
makes these the active settings. The rows and settings belong to the open identity and stay in its
folder; checking never changes the identity key.

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
