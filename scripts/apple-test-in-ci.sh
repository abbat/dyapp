#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [[ ${1:-} != --raw ]]; then
    exec python3 "$root/scripts/run-check.py" \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        apple-tests -- bash "$root/scripts/apple-test-in-ci.sh" --raw "$@"
fi
shift
if [[ ${GITHUB_ACTIONS:-false} != true ]]; then
    echo 'Native Apple tests require the approved macOS CI runner.' >&2
    exit 2
fi
python3 "$root/scripts/apple-test-in-ci.py" --raw
