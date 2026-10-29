#!/bin/bash
# Builds build/dyappd_<version>_<arch>.deb and build/dyappd-<version>-<machine>-linux-gnu.tar.gz
# with one release dyappd, and the same pair for the desktop client (dyapp_..., dyapp-...). Runs in the Debian 12 image (tools/docker/Dockerfile.deb), so the binary
# needs no newer glibc than Debian 12 has.
set -euo pipefail
cargo auditable build --release --locked --offline --package dyapp-bootstrap --bin dyappd
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(dpkg --print-architecture)
root=build/deb-root
rm -rf "$root"
install -D -s -m 0755 build/release/dyappd "$root/usr/bin/dyappd"
python3 -B tools/scripts/check-auditable.py "$root/usr/bin/dyappd" Cargo.lock
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
Depends: adduser, libc6 (>= 2.36), procps
Section: net
Priority: optional
Description: dyapp bootstrap node
 Serves signed profiles, per-device mailboxes and media blobs to dyapp
 clients over libp2p, as a hardened systemd service.
EOF
dpkg-deb --root-owner-group --build "$root" "build/dyappd_${version}_${arch}.deb"

# The tarball for a manual install: the same binary, the default config and the unit with the
# binary under /usr/local/bin. Sorted, root-owned and dated by the last commit, so it rebuilds
# byte for byte.
name="dyappd-$version-$(uname -m)-linux-gnu"
dir=build/tar-root/$name
rm -rf build/tar-root
install -D -m 0755 "$root/usr/bin/dyappd" "$dir/dyappd"
install -m 0644 rust/bootstrap/dyappd.toml LICENSE "$dir/"
sed 's|/usr/bin/dyappd|/usr/local/bin/dyappd|' packaging/debian/dyappd.service >"$dir/dyappd.service"
chmod 0644 "$dir/dyappd.service"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${SOURCE_DATE_EPOCH:-0}" -C build/tar-root \
    -cf - "$name" | gzip -n -9 >"build/$name.tar.gz"

# The desktop client: /usr/bin/dyapp with its menu entry and icon. Depends are what the binary
# links, as dpkg-shlibdeps finds them (WebKitGTK 4.1, GTK 3). The Tauri build script writes next to
# the manifest, so it builds from a writable copy of the sources it needs, outside the workspace.
src=$(mktemp -d)
cp -a Cargo.toml proto rust clients "$src/"
cargo auditable build --release --locked --offline \
    --manifest-path "$src/clients/linux/Cargo.toml" --target-dir build/client
client_version=$(sed -n 's/^version = "\(.*\)"/\1/p' clients/linux/Cargo.toml | head -n 1)
root=build/client-root
rm -rf "$root" build/shlibs
install -D -s -m 0755 build/client/release/dyapp-linux "$root/usr/bin/dyapp"
install -D -m 0644 packaging/linux/dyapp.desktop "$root/usr/share/applications/dyapp.desktop"
install -D -m 0644 packaging/linux/io.github.abbat.dyapp.metainfo.xml \
    "$root/usr/share/metainfo/io.github.abbat.dyapp.metainfo.xml"
install -D -m 0644 clients/linux/icons/icon.png "$root/usr/share/icons/hicolor/32x32/apps/dyapp.png"
mkdir -p build/shlibs/debian
printf 'Source: dyapp\n\nPackage: dyapp\nArchitecture: any\n' >build/shlibs/debian/control
depends=$(cd build/shlibs && dpkg-shlibdeps -O "$OLDPWD/$root/usr/bin/dyapp" |
    sed -n 's/^shlibs:Depends=//p')
mkdir -p "$root/DEBIAN"
cat >"$root/DEBIAN/control" <<EOF
Package: dyapp
Version: $client_version
Architecture: $arch
Maintainer: Anton Batenev <antonbatenev@yandex.ru>
Depends: $depends
Recommends: libayatana-appindicator3-1
Section: net
Priority: optional
Description: dyapp desktop client
 The dyapp client shell for Linux desktops.
EOF
dpkg-deb --root-owner-group --build "$root" "build/dyapp_${client_version}_${arch}.deb"

name="dyapp-$client_version-$(uname -m)-linux-gnu"
dir=build/tar-root/$name
install -D -m 0755 "$root/usr/bin/dyapp" "$dir/dyapp"
install -m 0644 packaging/linux/dyapp.desktop LICENSE "$dir/"
install -m 0644 clients/linux/icons/icon.png "$dir/dyapp.png"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${SOURCE_DATE_EPOCH:-0}" -C build/tar-root \
    -cf - "$name" | gzip -n -9 >"build/$name.tar.gz"
