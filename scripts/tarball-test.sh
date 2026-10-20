#!/bin/bash
# Runs the manual's tarball installation on a clean Debian 12, as far as a container without
# systemd allows: user, files and modes, then the node started as the unit's ExecStart would.
set -euo pipefail
cd /tmp
tar -xzf /deb/dyappd-*-linux-gnu.tar.gz
cd dyappd-*-linux-gnu
test "$(ls)" = "$(printf 'LICENSE\ndyappd\ndyappd.service\ndyappd.toml')"
# The manual's commands, in its order.
install -m 0755 dyappd /usr/local/bin/dyappd
install -m 0644 dyappd.toml /etc/dyappd.toml
mkdir -p /etc/systemd/system  # present on a host with systemd
install -m 0644 dyappd.service /etc/systemd/system/dyappd.service
useradd --system --user-group --home-dir /var/lib/dyappd --shell /usr/sbin/nologin dyappd
install -d -o dyappd -g dyappd -m 0750 /var/lib/dyappd
grep -qx "ExecStart=/usr/local/bin/dyappd --config /etc/dyappd.toml" \
    /etc/systemd/system/dyappd.service
grep -qx "User=dyappd" /etc/systemd/system/dyappd.service
# A plain command, so "${node[@]} &" leaves the node itself in $!.
node=(setpriv --reuid dyappd --regid dyappd --init-groups --no-new-privs \
        env RUST_LOG=info DYAPPD_NETWORK_ID_POW_BITS=8 \
        /usr/local/bin/dyappd --config /etc/dyappd.toml)
as_node() { "${node[@]}" "$@"; }
as_node keygen | grep -q "^12D3Koo"
test "$(stat -c %a /var/lib/dyappd/node.key)" = 600
"${node[@]}" >/tmp/node.log 2>&1 &
for _ in $(seq 30); do
    as_node status >/tmp/status.txt && break
    sleep 1
done
grep -qx "node running" /tmp/status.txt || { cat /tmp/node.log /tmp/status.txt; exit 1; }
kill $!
wait $! || true
if as_node status >/tmp/status.txt; then exit 1; fi
grep -qx "node stopped" /tmp/status.txt
echo "dyappd tarball installs on Debian 12 and runs as dyappd"
