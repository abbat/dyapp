#!/bin/bash
# One fresh emulator/width; preserve partial native evidence even on failure.
set -euo pipefail
bash /runner/tests/scripts/check-container-isolation.sh
width=${DYAPP_VISUAL_WIDTH:?}
source_sha=${DYAPP_VISUAL_SOURCE:?}
suite=${DYAPP_VISUAL_SUITE:-standard}
[[ $suite == standard || $suite == comparison || $suite == views || $suite == updates ]]
visual_archives=(native:files/owner-approval-visual ime:files/owner-approval-ime receive:files/receive-visual)
if [[ $suite == comparison ]]; then visual_archives=(comparison:files/comparison-visual); fi
if [[ $suite == views ]]; then visual_archives=(views:files/received-views-visual); fi
if [[ $suite == updates ]]; then visual_archives=(updates:files/update-notice-visual); fi
[[ $width == 360 || $width == 412 ]]
[[ $source_sha =~ ^[0-9a-f]{40}$ ]]
report=/reports/$width
mkdir -p "$report"
collect() {
    local code=$?
    timeout 30 adb -s emulator-5554 logcat -d > "$report/logcat.txt" 2>&1 || true
    for archive in "${visual_archives[@]}"; do
        if ! bash /runner/tests/scripts/android-visual-collect.sh \
            "$report" "${archive#*:}" "${archive%%:*}"; then
            if [[ $code == 0 ]]; then code=1; fi
        fi
    done
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
if [[ $suite == views || $suite == updates ]]; then
    visual_argument=viewsVisual visual_class=ReceivedViewsVisualTests
    if [[ $suite == updates ]]; then visual_argument=updatesVisual visual_class=UpdateNoticeVisualTests; fi
    python3 /runner/tests/scripts/android_instrumentation.py \
        --report "$report/$suite-instrumentation.txt" --deadline 900 --phase-callbacks -- \
        adb -s emulator-5554 shell am instrument -w -r \
        -e "$visual_argument" 1 -e sourceSha "$source_sha" -e expectedWidth "$width" \
        -e class "com.dyapp.$visual_class" com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
    python3 /runner/tests/scripts/check-instrumentation-results.py "$report/$suite-instrumentation.txt" 1
    exit 0
fi
if [[ $suite == comparison ]]; then
    python3 /runner/tests/scripts/android_instrumentation.py \
        --report "$report/comparison-instrumentation.txt" --deadline 900 --phase-callbacks -- \
        adb -s emulator-5554 shell am instrument -w -r \
        -e comparisonVisual 1 -e sourceSha "$source_sha" -e expectedWidth "$width" \
        -e class com.dyapp.LinkReviewVisualTests com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
    python3 /runner/tests/scripts/check-instrumentation-results.py "$report/comparison-instrumentation.txt" 2
    exit 0
fi
python3 /runner/tests/scripts/android_instrumentation.py \
    --report "$report/instrumentation.txt" --deadline 900 --phase-callbacks -- \
    adb -s emulator-5554 shell am instrument -w -r \
    -e ownerVisual 1 -e sourceSha "$source_sha" -e expectedWidth "$width" \
    -e class com.dyapp.OwnerApprovalVisualTests com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
python3 /runner/tests/scripts/check-instrumentation-results.py "$report/instrumentation.txt" 1
python3 /runner/tests/scripts/android_instrumentation.py \
    --report "$report/receive-instrumentation.txt" --deadline 900 --phase-callbacks -- \
    adb -s emulator-5554 shell am instrument -w -r \
    -e receiveVisual 1 -e sourceSha "$source_sha" -e expectedWidth "$width" \
    -e class com.dyapp.ReceiveVisualTests com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
python3 /runner/tests/scripts/check-instrumentation-results.py "$report/receive-instrumentation.txt" 1
timeout 30 adb -s emulator-5554 shell ime enable com.android.inputmethod.latin/.LatinIME
timeout 30 adb -s emulator-5554 shell ime set com.android.inputmethod.latin/.LatinIME
python3 /runner/tests/scripts/android_instrumentation.py \
    --report "$report/ime-instrumentation.txt" --deadline 900 --phase-callbacks -- \
    adb -s emulator-5554 shell am instrument -w -r \
    -e ownerIme 1 -e sourceSha "$source_sha" -e expectedWidth "$width" \
    -e class com.dyapp.OwnerApprovalImeTests com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
python3 /runner/tests/scripts/check-instrumentation-results.py "$report/ime-instrumentation.txt" 1
