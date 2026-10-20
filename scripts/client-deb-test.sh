#!/bin/bash
# Installs the desktop client .deb on a clean Debian 12, letting apt pull its Depends, then runs
# the desktop smoke (launch, render, close) on the installed binary and on the tarball's.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends /deb/dyapp_*.deb xvfb xauth dbus-x11 python3 \
    >/dev/null
test -f /usr/share/applications/dyapp.desktop
test -f /usr/share/icons/hicolor/32x32/apps/dyapp.png
cd /tmp
tar -xzf /deb/dyapp-*-linux-gnu.tar.gz
test "$(ls dyapp-*-linux-gnu)" = "$(printf 'LICENSE\ndyapp\ndyapp.desktop\ndyapp.png')"
useradd -m tester
for app in /usr/bin/dyapp /tmp/dyapp-*-linux-gnu/dyapp; do
    runuser -u tester -- env HOME=/home/tester RUNNER_TEMP=/tmp/reports \
        xvfb-run -a -s '-screen 0 1024x768x24' dbus-run-session -- \
        python3 /scripts/desktop-smoke.py "$app"
done
echo "dyapp client .deb and tarball install on Debian 12, launch and close"
