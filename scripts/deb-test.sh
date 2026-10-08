#!/bin/bash
# Installs the package on a clean Debian 12 and starts the node as the package's user.
set -euo pipefail
deb=$(ls /deb/dyapp-node_*.deb)
dpkg -i "$deb" >/dev/null
getent passwd dyapp-node | grep -q ':/var/lib/dyapp-node:/usr/sbin/nologin$'
test "$(stat -c %U /var/lib/dyapp-node)" = dyapp-node
# No systemd in the container: run what ExecStart runs, as User= would.
setpriv --reuid dyapp-node --regid dyapp-node --init-groups --no-new-privs \
    env RUST_LOG=info /usr/bin/dyapp-node --config /etc/dyapp-node.toml >/tmp/node.log 2>&1 &
sleep 3
kill $! || { cat /tmp/node.log; exit 1; }
grep -q "listening" /tmp/node.log || { cat /tmp/node.log; exit 1; }
test -s /var/lib/dyapp-node/node.key
dpkg -r dyapp-node >/dev/null
echo "dyapp-node package installs on Debian 12, runs as dyapp-node and removes"
