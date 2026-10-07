#!/bin/bash
set -euo pipefail
[[ $(id -u) != 0 ]] || { echo 'Runtime must not be root' >&2; exit 1; }
[[ -w ${HOME:?HOME must be set} ]] || { echo 'HOME must be writable' >&2; exit 1; }
capabilities=missing
privileges=missing
while IFS=$'\t' read -r key value; do
    case "$key" in
        CapEff:) capabilities=$value ;;
        NoNewPrivs:) privileges=$value ;;
    esac
done < /proc/self/status
[[ $capabilities == 0000000000000000 && $privileges == 1 ]] || {
    echo 'Expected zero effective capabilities and no-new-privileges' >&2
    exit 1
}
[[ ! -S /var/run/docker.sock && ! -S /tmp/.X11-unix/X0 ]] || {
    echo 'Host control/display sockets are forbidden' >&2
    exit 1
}
echo "Container isolation verified: uid=$(id -u), capabilities=$capabilities, NoNewPrivs=$privileges"
