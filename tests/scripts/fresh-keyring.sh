#!/bin/bash
# Start a credential provider under a fresh HOME and D-Bus session for the restore smoke.
set -euo pipefail
printf 'dyapp-ci-test-keyring' | gnome-keyring-daemon --unlock --components=secrets >/dev/null
exec "$@"
