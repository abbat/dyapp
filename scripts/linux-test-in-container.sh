#!/bin/bash
set -euo pipefail
bash /app/scripts/check-container-isolation.sh
mkdir -p /tmp/ai
python3 /app/scripts/run-cargo-unit-binaries.py /app/linux/unit-build.json
exec xvfb-run -a -s '-screen 0 1024x768x24' dbus-run-session -- \
    python3 /app/scripts/desktop-smoke.py /app/linux/target/debug/dyapp-linux
