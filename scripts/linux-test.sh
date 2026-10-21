#!/bin/bash
# Linux shell unit binaries and desktop smoke. The linux-test image builds them at image build
# time; on a CI runner this script builds them first.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [[ ${1:-} != --raw ]]; then
    exec python3 "$root/scripts/run-check.py" \
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
python3 "$root/scripts/run-cargo-unit-binaries.py" "$units"
export DYAPP_XVFB_DIAGNOSTICS=1
exec xvfb-run -a -e /dev/stderr -s '-screen 0 1024x768x24' dbus-run-session -- \
    python3 "$root/scripts/desktop-smoke.py" "$root/build/debug/dyapp-linux"
