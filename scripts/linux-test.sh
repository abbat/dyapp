#!/bin/bash
# Linux shell unit binaries and desktop smoke. The linux-test image builds them at image build
# time; on a CI runner this script builds them first.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$root/linux/Cargo.toml"
units="$root/linux/target/unit-build.json"
if [[ ! -f $units ]]; then
    cargo build --manifest-path "$manifest" --locked
    cargo test --manifest-path "$manifest" --locked --no-run --message-format=json > "$units"
fi
mkdir -p /tmp/ai
python3 "$root/scripts/run-cargo-unit-binaries.py" "$units"
exec xvfb-run -a -s '-screen 0 1024x768x24' dbus-run-session -- \
    python3 "$root/scripts/desktop-smoke.py" "$root/linux/target/debug/dyapp-linux"
