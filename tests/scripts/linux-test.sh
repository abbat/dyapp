#!/bin/bash
# Linux shell unit binaries and desktop smoke. The linux-test image builds them at image build
# time; on a CI runner this script builds them first.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
if [[ ${1:-} == --session ]]; then
    printf 'dyapp-ci-test-keyring' | gnome-keyring-daemon --unlock --components=secrets >/dev/null
    export DYAPP_TEST_SECRET_SERVICE=1
    python3 "$root/tests/scripts/run-cargo-unit-binaries.py" "$root/build/unit-build.json"
    exec python3 "$root/tests/scripts/desktop-smoke.py" "$root/build/debug/dyapp-linux"
fi
if [[ ${1:-} != --raw ]]; then
    exec python3 "$root/tools/scripts/run-check.py" \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        linux-test -- bash "$0" --raw "$@"
fi
shift
manifest="$root/clients/linux/Cargo.toml"
units="$root/build/unit-build.json"
if [[ ! -f $units ]]; then
    cargo build --manifest-path "$manifest" --locked
    cargo test --manifest-path "$manifest" --locked --no-run --message-format=json > "$units"
fi
mkdir -p /tmp/ai
export DYAPP_XVFB_DIAGNOSTICS=1
exec xvfb-run -a -e /dev/stderr -s '-screen 0 1024x768x24' dbus-run-session -- \
    bash "$0" --session
