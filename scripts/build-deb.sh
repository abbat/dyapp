#!/bin/bash
# Builds target/dyapp-node_<version>_<arch>.deb with a release dyapp-node. Runs in the Debian 12
# image (docker/Dockerfile.deb), so the binary needs no newer glibc than Debian 12 has.
set -euo pipefail
cargo build --release --locked --offline --package dyapp-bootstrap --bin dyapp-node
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(dpkg --print-architecture)
root=target/deb-root
rm -rf "$root"
install -D -s -m 0755 target/release/dyapp-node "$root/usr/bin/dyapp-node"
install -D -m 0644 packaging/debian/dyapp-node.service \
    "$root/lib/systemd/system/dyapp-node.service"
install -D -m 0644 packaging/debian/dyapp-node.toml "$root/etc/dyapp-node.toml"
install -D -m 0755 -t "$root/DEBIAN" \
    packaging/debian/postinst packaging/debian/prerm packaging/debian/postrm
echo /etc/dyapp-node.toml >"$root/DEBIAN/conffiles"
cat >"$root/DEBIAN/control" <<EOF
Package: dyapp-node
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
dpkg-deb --root-owner-group --build "$root" "target/dyapp-node_${version}_${arch}.deb"
