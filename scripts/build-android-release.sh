#!/bin/bash
# Build unsigned Release packages in a writable source copy with prepared caches.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
reports=${1:?usage: build-android-release.sh REPORT_DIRECTORY}
mkdir -p "$reports"
reports=$(cd "$reports" && pwd)
export CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$root/target"
cd "$root/android"
if sh ./gradlew --offline --no-daemon :app:assembleRelease :app:bundleRelease \
    > "$reports/build.log" 2>&1; then
    python3 "$root/scripts/check-android-release.py" \
        --apk app/build/outputs/apk/release/app-release-unsigned.apk \
        --aab app/build/outputs/bundle/release/app-release.aab \
        --report "$reports/packages.json"
else
    code=$?
    printf 'FAIL android-release: Gradle exited with %s\n' "$code"
    echo "FIX: docs/development/build.md#unsigned-android-release"
    echo "DETAILS:"
    echo "$reports/build.log"
    exit "$code"
fi
