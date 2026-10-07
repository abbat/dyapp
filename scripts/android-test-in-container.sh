#!/bin/bash
set -euo pipefail
bash /app/scripts/check-container-isolation.sh
mkdir -p /tmp/ai
case "${1:?suite required}" in
    unit)
        cp -rf /app/android /tmp/ai/android
        cp -rf /opt/gradle-cache /tmp/ai/gradle-cache
        export GRADLE_USER_HOME=/tmp/ai/gradle-cache
        cd /tmp/ai/android
        report_dir="/reports/$HOSTNAME"
        mkdir -p "$report_dir"
        trap 'code=$?; for crash in /tmp/ai/android/hs_err*.log; do if [[ -f $crash ]]; then head -60 "$crash"; cp -f "$crash" "$report_dir/"; fi; done; cp -rf app/build/test-results "$report_dir/" 2>/dev/null || true; exit "$code"' EXIT
        sh ./gradlew --offline --no-daemon --version > "$report_dir/gradle-version.txt"
        cat "$report_dir/gradle-version.txt"
        sh ./gradlew --offline --no-daemon tasks --all > "$report_dir/gradle-tasks.txt"
        sh ./gradlew --offline --no-daemon --rerun-tasks :app:testDebugUnitTest
        python3 /app/scripts/check-junit-results.py app/build/test-results/testDebugUnitTest
        ;;
    emulator)
        export ANDROID_USER_HOME=/home/appuser/.android
        mkdir -p "$ANDROID_USER_HOME"
        # /avd is a disk volume (see compose.ui.yml); --force drops the previous run's AVD.
        echo no | avdmanager create avd --force -n test_device -k 'system-images;android-30;default;x86_64' -p /avd/test_device
        adb -a -P 5037 nodaemon server > /tmp/ai/adb.log 2>&1 &
        exec emulator -avd test_device -port 5554 -accel "${EMULATOR_ACCEL:-off}" -no-window -no-audio -no-metrics \
            -no-boot-anim -no-snapshot -gpu swiftshader_indirect -memory 2048 -cores 2 \
            -feature -Vulkan
        ;;
    ui)
        report_dir="/reports/$HOSTNAME"
        mkdir -p "$report_dir"
        collect_ui_reports() {
            code=$?
            if [[ $code != 0 ]]; then
                timeout 60 adb -s emulator-5554 exec-out screencap -p \
                    > "$report_dir/failure.png" 2>/dev/null || true
                timeout 30 adb -s emulator-5554 shell dumpsys window \
                    > "$report_dir/window.txt" 2>&1 || true
            fi
            timeout 30 adb -s emulator-5554 logcat -d \
                > "$report_dir/logcat.txt" 2>&1 || true
            exit "$code"
        }
        trap collect_ui_reports EXIT
        timeout 300 adb -s emulator-5554 install --no-streaming -r /app/android/app/build/outputs/apk/debug/app-debug.apk
        timeout 300 adb -s emulator-5554 install --no-streaming -r /app/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
        timeout 900 adb -s emulator-5554 shell am instrument -w com.dyapp.test/androidx.test.runner.AndroidJUnitRunner > "$report_dir/instrumentation.txt"
        python3 /app/scripts/check-instrumentation-results.py "$report_dir/instrumentation.txt"
        timeout 60 adb -s emulator-5554 exec-out run-as com.dyapp cat files/ui-ready.png > "$report_dir/ready.png"
        test -s "$report_dir/ready.png"
        ;;
    *) echo "Unknown suite: $1" >&2; exit 2 ;;
esac
