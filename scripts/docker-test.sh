#!/bin/bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
if [[ ${1:-} == --raw ]]; then
    shift
elif [[ ${DYAPP_VERBOSE:-0} != 1 ]]; then
    case "${1:-test}" in
        help|--help|-h|shell) ;;
        *) exec python3 scripts/run-check.py \
            --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
            docker-tests -- bash "$PWD/scripts/docker-test.sh" --raw "$@" ;;
    esac
fi
run() {
    python3 scripts/docker-local.py -f docker/compose.dev.yml --profile "$1" run --no-deps --pull never "$2"
}
case "${1:-test}" in
    test) run test dev ;;
    build|quality|coverage|security) run "$1" "$1" ;;
    prepare) python3 scripts/docker-local.py -f docker/compose.dev.yml --profile test build dev ;;
    network-prepare) python3 scripts/docker-local.py -f docker/compose.network.yml build network-test ;;
    network) python3 scripts/docker-local.py -f docker/compose.network.yml run --pull never network-test ;;
    all)
        run build build
        run quality quality
        run test dev
        run coverage coverage
        run security security
        python3 scripts/docker-local.py -f docker/compose.network.yml run --pull never network-test
        bash scripts/ui-test.sh --raw all
        ;;
    shell) DYAPP_VERBOSE=1 python3 scripts/docker-local.py -f docker/compose.dev.yml --profile test run --no-deps dev bash ;;
    help|--help|-h) echo 'Usage: docker-test.sh test|build|quality|coverage|security|prepare|network|network-prepare|all|shell' ;;
    *) echo "Unknown command: $1" >&2; exit 2 ;;
esac
