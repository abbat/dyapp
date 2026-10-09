#!/bin/bash
# Builds target/dyappd_<version>_<arch>.deb with a release dyappd. Runs in the Debian 12
# image (docker/Dockerfile.deb), so the binary needs no newer glibc than Debian 12 has.
set -euo pipefail
cargo build --release --locked --offline --package dyapp-bootstrap --bin dyappd
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(dpkg --print-architecture)
root=target/deb-root
rm -rf "$root"
install -D -s -m 0755 target/release/dyappd "$root/usr/bin/dyappd"
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
