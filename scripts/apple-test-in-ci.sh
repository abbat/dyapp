#!/bin/bash
set -euo pipefail
if [[ ${GITHUB_ACTIONS:-false} != true ]]; then
    echo 'Native Apple tests require the approved macOS CI runner.' >&2
    exit 2
fi
python3 scripts/apple-test-in-ci.py
