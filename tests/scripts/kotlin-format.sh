#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"
cli=${DYAPP_DETEKT_CLI:-/usr/local/lib/detekt-cli.jar}
formatting=${DYAPP_DETEKT_FORMATTING:-/usr/local/lib/detekt-formatting.jar}
java_command=java
if [[ -n ${JAVA_HOME:-} ]]; then
    java_command="$JAVA_HOME/bin/java"
fi
command=("$java_command" -jar "$cli" --plugins "$formatting" --config detekt.yml
    --baseline detekt-baseline.xml --input clients/android
    --excludes '**/clients/android/**/build/**' --build-upon-default-config)
case "${1:-}" in
    lint) exec "${command[@]}" ;;
    format)
        # Wrapping can expose a second indentation correction; require convergence.
        for ((pass = 0; pass < 3; pass++)); do
            status=0
            "${command[@]}" --auto-correct || status=$?
            if [[ $status == 0 ]]; then
                exec "${command[@]}"
            fi
            if [[ $status != 2 ]]; then
                exit "$status"
            fi
        done
        exec "${command[@]}"
        ;;
    *) echo "Usage: $0 lint|format" >&2; exit 2 ;;
esac
