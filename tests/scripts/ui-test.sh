#!/bin/bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
quiet() {
    exec python3 tools/scripts/run-check.py \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        ui-tests -- bash "$PWD/tests/scripts/ui-test.sh" --raw "$@"
}
if [[ ${1:-} == --raw ]]; then
    shift
elif [[ ${DYAPP_VERBOSE:-0} != 1 ]]; then
    case "${1:-help}" in
        help|--help|-h) ;;
        shell) case "${2:-android}" in android|linux) ;; *) quiet "$@" ;; esac ;;
        *) quiet "$@" ;;
    esac
fi
case "${1:-help}" in
    help|--help|-h)
        echo 'Usage: ui-test.sh android|android-emulator|linux|all|prepare|shell [platform]'
        exit 0 ;;
    android|android-emulator|linux|all|prepare) ;;
    shell) case "${2:-android}" in
        android|linux) ;;
        *) echo "Unknown platform: $2" >&2; exit 2 ;;
    esac ;;
    *) echo "Unknown or unsupported command: $1" >&2; exit 2 ;;
esac
compose=(-f tests/docker/compose.ui.yml)
# DYAPP_KVM=1 runs the emulator with KVM; the default is software emulation.
if [[ ${DYAPP_KVM:-0} == 1 ]]; then
    compose+=(-f tests/docker/compose.kvm.yml)
    DYAPP_KVM_GID=$(stat -c %g /dev/kvm)
    export DYAPP_KVM_GID
fi
# The Linux run saves its screenshots here; stale ones would be kept, so start empty.
shots() { mkdir -p build/ux-research/current && rm -f build/ux-research/current/*.png; }
run() {
    python3 tools/scripts/docker-local.py "${compose[@]}" --profile all run --pull never "$1"
}
case "${1:-help}" in
    android) run android-unit-test ;;
    android-emulator) run android-emulator-test ;;
    linux) shots; run linux-test ;;
    all)
        run android-unit-test
        run android-emulator-test
        shots
        run linux-test
        echo 'Local Docker UI inventory passed; Apple/Windows native CI is required separately.'
        ;;
    prepare) python3 tools/scripts/docker-local.py -f tests/docker/compose.ui.yml --profile all build ;;
    shell)
        case "${2:-android}" in
            android) service=android-unit-test ;;
            linux) service=linux-test ;;
            *) echo "Unknown platform: $2" >&2; exit 2 ;;
        esac
        DYAPP_VERBOSE=1 python3 tools/scripts/docker-local.py -f tests/docker/compose.ui.yml --profile all run --no-deps "$service" bash
        ;;
    help|--help|-h) echo 'Usage: ui-test.sh android|android-emulator|linux|all|prepare|shell [platform]' ;;
    *) echo "Unknown or unsupported command: $1" >&2; exit 2 ;;
esac
