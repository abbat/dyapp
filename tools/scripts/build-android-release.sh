#!/bin/bash
# Build unsigned Release packages in a writable source copy with prepared caches.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
if [[ ${1:-} == --raw ]]; then
    shift
elif [[ ${1:-} != --help && ${1:-} != -h ]]; then
    exec python3 "$root/tools/scripts/run-check.py" \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        android-build -- bash "$root/tools/scripts/build-android-release.sh" --raw "$@"
fi
if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    echo "Usage: build-android-release.sh REPORT_DIRECTORY"
    exit 0
fi
reports=${1:?usage: build-android-release.sh REPORT_DIRECTORY}
mkdir -p "$reports"
reports=$(cd "$reports" && pwd)
export CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$root/build"
cd "$root/clients/android"
gradle_args=(--offline --no-daemon)
if [[ ${DYAPP_ANDROID_FRESH:-0} == 1 ]]; then
    gradle_args+=(--rerun-tasks --no-build-cache --no-configuration-cache
        --project-cache-dir "$root/build/gradle-project-cache" -Pkotlin.incremental=false)
fi
if python3 "$root/tools/scripts/android_release_environment.py" "$root" \
    sh ./gradlew "${gradle_args[@]}" :app:assembleRelease :app:bundleRelease \
    > "$reports/build.log" 2>&1; then
    apk="$root/build/android/app/outputs/apk/release/app-release-unsigned.apk"
    aab="$root/build/android/app/outputs/bundle/release/app-release.aab"
    python3 "$root/tools/scripts/check-android-release.py" --apk "$apk" --aab "$aab" \
        --report "$reports/packages.json"
    python3 "$root/tools/scripts/check-hardening.py" --android-apk "$apk"
    zipalign=$(find "${ANDROID_SDK_ROOT:?}/build-tools" -mindepth 2 -maxdepth 2 \
        -type f -name zipalign | sort -V | tail -n 1)
    test -x "$zipalign"
    "$zipalign" -c -P 16 4 "$apk"
    echo 'OK android ZIP alignment'
    # Only fully verified packages leave the container for the size/signing gates.
    cp -f "$apk" "$aab" "$reports/"
else
    code=$?
    printf 'FAIL android-release: Gradle exited with %s\n' "$code"
    echo "FIX: docs/development/build.md#unsigned-android-release"
    echo "DETAILS:"
    echo "$reports/build.log"
    exit "$code"
fi
