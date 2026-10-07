#!/bin/bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
compose=(-f docker/compose.ui.yml)
# DYAPP_KVM=1 runs the emulator with KVM; the default is software emulation.
if [[ ${DYAPP_KVM:-0} == 1 ]]; then
    compose+=(-f docker/compose.kvm.yml)
    DYAPP_KVM_GID=$(stat -c %g /dev/kvm)
    export DYAPP_KVM_GID
fi
run() {
    python3 scripts/docker-local.py "${compose[@]}" --profile all run --pull never "$1"
}
case "${1:-help}" in
    android) run android-unit-test ;;
    android-emulator) run android-emulator-test ;;
    linux) run linux-test ;;
    all)
        run android-unit-test
        run android-emulator-test
        run linux-test
        echo 'Local Docker UI inventory passed; Apple/Windows native CI is required separately.'
        ;;
    prepare) python3 scripts/docker-local.py -f docker/compose.ui.yml --profile all build ;;
    shell)
        case "${2:-android}" in
            android) service=android-unit-test ;;
            linux) service=linux-test ;;
            *) echo "Unknown platform: $2" >&2; exit 2 ;;
        esac
        python3 scripts/docker-local.py -f docker/compose.ui.yml --profile all run --no-deps "$service" bash
        ;;
    help|--help|-h) echo 'Usage: ui-test.sh android|android-emulator|linux|all|prepare|shell [platform]' ;;
    *) echo "Unknown or unsupported command: $1" >&2; exit 2 ;;
esac
