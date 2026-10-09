#!/bin/bash
# Installs the package on a clean Debian 12 and starts the node as the package's user.
set -euo pipefail
deb=$(ls /deb/dyappd_*.deb)
dpkg -i "$deb" >/dev/null
getent passwd dyappd | grep -q ':/var/lib/dyappd:/usr/sbin/nologin$'
test "$(stat -c %U /var/lib/dyappd)" = dyappd
# No systemd in the container: run what ExecStart runs, as User= would. A low proof of work keeps
# keygen fast.
as_node() {
    setpriv --reuid dyappd --regid dyappd --init-groups --no-new-privs \
        env RUST_LOG=info DYAPPD__NETWORK__ID_POW_BITS=8 /usr/bin/dyappd "$@" --config /etc/dyappd.toml
}
if as_node >/tmp/node.log 2>&1; then exit 1; fi
grep -q "dyappd keygen" /tmp/node.log || { cat /tmp/node.log; exit 1; }
as_node keygen >/dev/null
as_node >/tmp/node.log 2>&1 &
sleep 3
kill $! || { cat /tmp/node.log; exit 1; }
grep -q "listening" /tmp/node.log || { cat /tmp/node.log; exit 1; }
test -s /var/lib/dyappd/node.key
dpkg -r dyappd >/dev/null
echo "dyappd package installs on Debian 12, runs as dyappd and removes"
