#!/bin/bash
set -euo pipefail
fail() {
    printf 'FAIL container-isolation: %s\nFIX: docs/testing/docker.md#container-isolation\nDETAILS:\n%s\n' "$1" "$1"
    exit 1
}
[[ $(id -u) != 0 ]] || fail 'Runtime must not be root'
[[ -w ${HOME:-} ]] || fail 'HOME must be writable'
capabilities=missing
privileges=missing
while IFS=$'\t' read -r key value; do
    case "$key" in
        CapEff:) capabilities=$value ;;
        NoNewPrivs:) privileges=$value ;;
    esac
done < /proc/self/status
[[ $capabilities == 0000000000000000 && $privileges == 1 ]] || {
    fail 'Expected zero effective capabilities and no-new-privileges'
}
[[ ! -S /var/run/docker.sock && ! -S /tmp/.X11-unix/X0 ]] || {
    fail 'Host control/display sockets are forbidden'
}
if [[ ${1:-} == --raw ]]; then
    echo "Container isolation verified: uid=$(id -u), capabilities=$capabilities, NoNewPrivs=$privileges"
else
    echo "OK container-isolation"
fi
