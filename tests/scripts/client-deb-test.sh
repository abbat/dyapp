#!/bin/bash
# Installs the desktop client .deb on a clean Debian 12, letting apt pull its Depends, then runs
# the desktop smoke (launch, render, close) on the installed binary and on the tarball's.
set -euo pipefail
trap 'printf "client-deb-test failed at line %s\n" "$LINENO" >&2' ERR
if [[ ${1:-} == --session ]]; then
    printf 'dyapp-ci-test-keyring' | gnome-keyring-daemon --unlock --components=secrets >/dev/null
    exec python3 /tests/scripts/desktop-smoke.py "$2"
fi
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends /deb/dyapp_*.deb xvfb xauth dbus-x11 gnome-keyring python3 \
    >/dev/null
test -f /usr/share/applications/dyapp.desktop
test -f /usr/share/metainfo/io.github.abbat.dyapp.metainfo.xml
test -f /usr/share/icons/hicolor/32x32/apps/dyapp.png
python3 - <<'PY'
import configparser
import xml.etree.ElementTree as ET

desktop = configparser.ConfigParser(interpolation=None)
desktop.read('/usr/share/applications/dyapp.desktop', encoding='utf-8')
assert desktop['Desktop Entry']['Comment[ru]']
root = ET.parse('/usr/share/metainfo/io.github.abbat.dyapp.metainfo.xml').getroot()
assert root.findtext('launchable') == 'dyapp.desktop'
summaries = {tag.get('{http://www.w3.org/XML/1998/namespace}lang', 'en'): tag.text
             for tag in root.findall('summary')}
assert summaries['en'] and summaries['ru']
PY
cd /tmp
tar -xzf /deb/dyapp-*-linux-gnu.tar.gz
test "$(ls dyapp-*-linux-gnu)" = "$(printf 'LICENSE\ndyapp\ndyapp.desktop\ndyapp.png')"
apps=(/usr/bin/dyapp /tmp/dyapp-*-linux-gnu/dyapp)
users=(tester-deb tester-tarball)
for index in "${!apps[@]}"; do
    useradd -m "${users[index]}"
    runuser -u "${users[index]}" -- env \
        HOME="/home/${users[index]}" RUNNER_TEMP="/tmp/reports-${users[index]}" \
        DYAPP_CHECK_LOG_DIR="/tmp/reports-${users[index]}/check-output" \
        DYAPP_XVFB_DIAGNOSTICS=1 \
        xvfb-run -a -s '-screen 0 1024x768x24' dbus-run-session -- \
        bash /tests/scripts/client-deb-test.sh --session "${apps[index]}"
done
echo "dyapp client .deb and tarball install on Debian 12, launch and close"
