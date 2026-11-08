#!/bin/bash
# Linux shell unit binaries and desktop smoke, built first (a no-op when the build is current).
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
if [[ ${1:-} == --session ]]; then
    printf 'dyapp-ci-test-keyring' | gnome-keyring-daemon --unlock --components=secrets >/dev/null
    export DYAPP_TEST_SECRET_SERVICE=1
    python3 "$root/tests/scripts/run-cargo-unit-binaries.py" "$root/build/unit-build.json"
    DYAPP_VIDEO_SMOKE=1 python3 "$root/tests/scripts/desktop-smoke.py" "$root/build/debug/dyapp-linux"
    # With screenshots on (DYAPP_UI_SHOTS), a second pass in Russian saves the -ru screenshots.
    if [[ -n ${DYAPP_UI_SHOTS:-} ]]; then
        DYAPP_SMOKE_RU=1 DYAPP_VIDEO_SMOKE=1 python3 "$root/tests/scripts/desktop-smoke.py" "$root/build/debug/dyapp-linux"
    fi
    # Last, as both take the keyring away.
    reach=(python3 "$root/tests/scripts/run-cargo-unit-binaries.py" "$root/build/unit-build.json"
           controlled_service_out_of_reach)
    DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent DYAPP_TEST_SECRET_SERVICE=absent "${reach[@]}"
    DYAPP_TEST_SECRET_SERVICE=locked "${reach[@]}"
    exit 0
fi
if [[ ${1:-} != --raw ]]; then
    exec python3 "$root/tools/scripts/run-check.py" \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        linux-test -- bash "$0" --raw "$@"
fi
shift
manifest="$root/clients/linux/Cargo.toml"
units="$root/build/unit-build.json"
python3 -B "$root/tests/scripts/check-rust-complexity.py" --scope linux
if [[ ${1:-} == --lint ]]; then
    exit 0
fi
cargo build --manifest-path "$manifest" --locked
if ! cargo test --manifest-path "$manifest" --locked --no-run --message-format=json > "$units"; then
    python3 -B "$root/tests/scripts/cargo-errors.py" "$units" >&2
    exit 1
fi
# The desktop smoke checks the connection form against a loopback dyappd.
cargo build --manifest-path "$root/Cargo.toml" --locked -p dyapp-bootstrap --bin dyappd
mkdir -p /tmp/ai
export DYAPP_XVFB_DIAGNOSTICS=1
# The desktop smoke audits every screenshot point with axe-core (ci-setup.sh and the test image).
export DYAPP_AXE=${DYAPP_AXE:-/opt/axe-core/axe.min.js}
exec xvfb-run -a -e /dev/stderr -s '-screen 0 1280x1400x24' dbus-run-session -- \
    bash "$0" --session
