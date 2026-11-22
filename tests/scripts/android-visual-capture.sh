#!/bin/bash
# One fresh emulator/width; preserve partial native evidence even on failure.
set -euo pipefail
bash /runner/tests/scripts/check-container-isolation.sh
width=${DYAPP_VISUAL_WIDTH:?}
source_sha=${DYAPP_VISUAL_SOURCE:?}
[[ $width == 360 || $width == 412 ]]
[[ $source_sha =~ ^[0-9a-f]{40}$ ]]
report=/reports/$width
mkdir -p "$report"
collect() {
    local code=$?
    timeout 30 adb -s emulator-5554 logcat -d > "$report/logcat.txt" 2>&1 || true
    timeout 60 adb -s emulator-5554 exec-out run-as com.dyapp \
        tar -cf - files/owner-approval-visual > "$report/native.tar" 2> "$report/collection-error.txt" || true
    timeout 60 adb -s emulator-5554 exec-out run-as com.dyapp \
        tar -cf - files/owner-approval-ime > "$report/ime.tar" 2> "$report/ime-collection-error.txt" || true
    exit "$code"
}
trap collect EXIT
python3 -B /runner/tests/scripts/android_runtime_inventory.py \
    --output "$report/android-artifacts.json" \
    --application /app/build/android/lifecycle/app-base.apk \
    --test /app/build/android/app/outputs/apk/androidTest/debug/app-debug-androidTest.apk
timeout 300 adb -s emulator-5554 install --no-streaming /app/build/android/lifecycle/app-base.apk
timeout 300 adb -s emulator-5554 install --no-streaming \
    /app/build/android/app/outputs/apk/androidTest/debug/app-debug-androidTest.apk
timeout 30 adb -s emulator-5554 shell wm density > "$report/density.txt"
timeout 30 adb -s emulator-5554 shell wm size > "$report/size-before.txt"
density=$(tr -d '\r' < "$report/density.txt" | sed -nE 's/^(Physical|Override) density: ([0-9]+)$/\2/p' | tail -1)
height=$(tr -d '\r' < "$report/size-before.txt" | sed -nE 's/^(Physical|Override) size: [0-9]+x([0-9]+)$/\2/p' | tail -1)
[[ $density =~ ^[1-9][0-9]*$ && $height =~ ^[1-9][0-9]*$ ]]
pixels=$(( (width * density + 159) / 160 ))
timeout 30 adb -s emulator-5554 shell wm size "${pixels}x$height"
timeout 30 adb -s emulator-5554 shell wm size > "$report/size-after.txt"
timeout 30 adb -s emulator-5554 shell input keyevent KEYCODE_WAKEUP
timeout 30 adb -s emulator-5554 shell wm dismiss-keyguard
timeout 30 adb -s emulator-5554 shell ime disable com.android.inputmethod.latin/.LatinIME
python3 /runner/tests/scripts/android_instrumentation.py \
    --report "$report/instrumentation.txt" --deadline 900 -- \
    adb -s emulator-5554 shell am instrument -w -r \
    -e ownerVisual 1 -e sourceSha "$source_sha" -e expectedWidth "$width" \
    -e class com.dyapp.OwnerApprovalVisualTests com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
python3 /runner/tests/scripts/check-instrumentation-results.py "$report/instrumentation.txt" 1
timeout 30 adb -s emulator-5554 shell ime enable com.android.inputmethod.latin/.LatinIME
timeout 30 adb -s emulator-5554 shell ime set com.android.inputmethod.latin/.LatinIME
python3 /runner/tests/scripts/android_instrumentation.py \
    --report "$report/ime-instrumentation.txt" --deadline 900 -- \
    adb -s emulator-5554 shell am instrument -w -r \
    -e ownerIme 1 -e sourceSha "$source_sha" -e expectedWidth "$width" \
    -e class com.dyapp.OwnerApprovalImeTests com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
python3 /runner/tests/scripts/check-instrumentation-results.py "$report/ime-instrumentation.txt" 1
