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
        env RUST_LOG=info DYAPPD_NETWORK_ID_POW_BITS=8 CREDENTIALS_DIRECTORY=/run/creds \
        /usr/bin/dyappd "$@" --config /etc/dyappd.toml
}
if as_node >/tmp/node.log 2>&1; then exit 1; fi
grep -q "dyappd keygen" /tmp/node.log || { cat /tmp/node.log; exit 1; }
# The packaged default: the key piped to systemd-creds, which the unit decrypts into a credential
# readable by the service user only. Here the pipe ends in that credential directly.
grep -qx LoadCredentialEncrypted=node.key /lib/systemd/system/dyappd.service
mkdir -p /run/creds
as_node keygen - >/run/creds/node.key 2>/tmp/keygen.log
grep -q "^12D3Koo" /tmp/keygen.log
chgrp dyappd /run/creds/node.key
chmod 0440 /run/creds/node.key
test ! -e /var/lib/dyappd/node.key
as_node >/tmp/node.log 2>&1 &
sleep 3
# Not dumpable: /proc/<pid> of such a process belongs to root.
test "$(stat -c %U /proc/$!)" = root || { cat /tmp/node.log; exit 1; }
grep -qx LimitCORE=0 /lib/systemd/system/dyappd.service
# The running node picks up a deny entry within 10 seconds; root may not touch the stores.
as_node deny add 203.0.113.9/24 deb test >/dev/null
if /usr/bin/dyappd status --config /etc/dyappd.toml 2>/dev/null; then exit 1; fi
sleep 11
kill $! || { cat /tmp/node.log; exit 1; }
grep -q "listening" /tmp/node.log || { cat /tmp/node.log; exit 1; }
# Loaded at start and once after the change.
test "$(grep -c "deny list loaded" /tmp/node.log)" = 2 || { cat /tmp/node.log; exit 1; }
as_node deny list | grep -q "^203.0.113.0/24	.*	deb test$"
as_node status | grep -q "^deny_entries 1$"
test "$(cat /var/lib/dyappd/node.id)" = "$(cat /tmp/keygen.log)"
dpkg -r dyappd >/dev/null
echo "dyappd package installs on Debian 12, runs as dyappd and removes"
