#!/bin/bash
# Builds target/dyappd_<version>_<arch>.deb and target/dyappd-<version>-<machine>-linux-gnu.tar.gz
# with one release dyappd, and the same pair for the desktop client (dyapp_..., dyapp-...). Runs in the Debian 12 image (docker/Dockerfile.deb), so the binary
# needs no newer glibc than Debian 12 has.
set -euo pipefail
cargo auditable build --release --locked --offline --package dyapp-bootstrap --bin dyappd
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(dpkg --print-architecture)
root=target/deb-root
rm -rf "$root"
install -D -s -m 0755 target/release/dyappd "$root/usr/bin/dyappd"
python3 -B scripts/check-auditable.py "$root/usr/bin/dyappd" Cargo.lock
install -D -m 0644 packaging/debian/dyappd.service \
    "$root/lib/systemd/system/dyappd.service"
install -D -m 0644 rust/bootstrap/dyappd.toml "$root/etc/dyappd.toml"
install -D -m 0755 -t "$root/DEBIAN" \
    packaging/debian/postinst packaging/debian/prerm packaging/debian/postrm
echo /etc/dyappd.toml >"$root/DEBIAN/conffiles"
cat >"$root/DEBIAN/control" <<EOF
Package: dyappd
Version: $version
Architecture: $arch
Maintainer: Anton Batenev <antonbatenev@yandex.ru>
Depends: adduser, libc6 (>= 2.36)
Section: net
Priority: optional
Description: dyapp bootstrap node
 Serves signed profiles, per-device mailboxes and media blobs to dyapp
 clients over libp2p, as a hardened systemd service.
EOF
dpkg-deb --root-owner-group --build "$root" "target/dyappd_${version}_${arch}.deb"

# The tarball for a manual install: the same binary, the default config and the unit with the
# binary under /usr/local/bin. Sorted, root-owned and dated by the last commit, so it rebuilds
# byte for byte.
name="dyappd-$version-$(uname -m)-linux-gnu"
dir=target/tar-root/$name
rm -rf target/tar-root
install -D -m 0755 "$root/usr/bin/dyappd" "$dir/dyappd"
install -m 0644 rust/bootstrap/dyappd.toml LICENSE "$dir/"
sed 's|/usr/bin/dyappd|/usr/local/bin/dyappd|' packaging/debian/dyappd.service >"$dir/dyappd.service"
chmod 0644 "$dir/dyappd.service"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${SOURCE_DATE_EPOCH:-0}" -C target/tar-root \
    -cf - "$name" | gzip -n -9 >"target/$name.tar.gz"

# The desktop client: /usr/bin/dyapp with its menu entry and icon. Depends are what the binary
# links, as dpkg-shlibdeps finds them (WebKitGTK 4.1, GTK 3). The Tauri build script writes next to
# the manifest, so it builds from a writable copy of the sources it needs, outside the workspace.
src=$(mktemp -d)
cp -a Cargo.toml proto rust linux desktop "$src/"
cargo auditable build --release --locked --offline \
    --manifest-path "$src/linux/Cargo.toml" --target-dir target/client
client_version=$(sed -n 's/^version = "\(.*\)"/\1/p' linux/Cargo.toml | head -n 1)
root=target/client-root
rm -rf "$root" target/shlibs
install -D -s -m 0755 target/client/release/dyapp-linux "$root/usr/bin/dyapp"
install -D -m 0644 packaging/linux/dyapp.desktop "$root/usr/share/applications/dyapp.desktop"
install -D -m 0644 linux/icons/icon.png "$root/usr/share/icons/hicolor/32x32/apps/dyapp.png"
mkdir -p target/shlibs/debian
printf 'Source: dyapp\n\nPackage: dyapp\nArchitecture: any\n' >target/shlibs/debian/control
depends=$(cd target/shlibs && dpkg-shlibdeps -O "$OLDPWD/$root/usr/bin/dyapp" |
    sed -n 's/^shlibs:Depends=//p')
mkdir -p "$root/DEBIAN"
cat >"$root/DEBIAN/control" <<EOF
Package: dyapp
Version: $client_version
Architecture: $arch
Maintainer: Anton Batenev <antonbatenev@yandex.ru>
Depends: $depends
Section: net
Priority: optional
Description: dyapp desktop client
 The dyapp client shell for Linux desktops.
EOF
dpkg-deb --root-owner-group --build "$root" "target/dyapp_${client_version}_${arch}.deb"

name="dyapp-$client_version-$(uname -m)-linux-gnu"
dir=target/tar-root/$name
install -D -m 0755 "$root/usr/bin/dyapp" "$dir/dyapp"
install -m 0644 packaging/linux/dyapp.desktop LICENSE "$dir/"
install -m 0644 linux/icons/icon.png "$dir/dyapp.png"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${SOURCE_DATE_EPOCH:-0}" -C target/tar-root \
    -cf - "$name" | gzip -n -9 >"target/$name.tar.gz"
