#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
if [[ ${1:-} == --raw ]]; then
    shift
elif [[ ${DYAPP_VERBOSE:-0} != 1 ]]; then
    exec python3 "$root/tools/scripts/run-check.py" \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        check-quality -- bash "$root/tests/scripts/check-quality.sh" --raw "$@"
fi
case "${1:-}" in
    '') exec bash "$(dirname "${BASH_SOURCE[0]}")/docker-test.sh" --raw quality ;;
    --coverage) exec bash "$(dirname "${BASH_SOURCE[0]}")/docker-test.sh" --raw coverage ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
esac
