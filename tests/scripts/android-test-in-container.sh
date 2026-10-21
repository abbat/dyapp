#!/bin/bash
set -euo pipefail
bash /app/tests/scripts/check-container-isolation.sh
mkdir -p /tmp/ai
export DYAPP_CHECK_LOG_DIR="/reports/$HOSTNAME/check-output"
case "${1:?suite required}" in
    unit)
        cp -rf /app/clients/android /tmp/ai/android
        mkdir -p /tmp/build/android
        cp -rf /app/build/android/app /tmp/build/android/app
        cp -rf /opt/gradle-cache /tmp/ai/gradle-cache
        export GRADLE_USER_HOME=/tmp/ai/gradle-cache
        cd /tmp/ai/android
        report_dir="/reports/$HOSTNAME"
        mkdir -p "$report_dir"
        save_reports() {
            local code=$? crash
            for crash in /tmp/ai/android/hs_err*.log; do
                if [[ -f $crash ]]; then head -60 "$crash"; cp -f "$crash" "$report_dir/"; fi
            done
            cp -rf /tmp/build/android/app/test-results "$report_dir/" 2>/dev/null || true
            exit "$code"
        }
        trap save_reports EXIT
        sh ./gradlew --offline --no-daemon --version > "$report_dir/gradle-version.txt"
        cat "$report_dir/gradle-version.txt"
        sh ./gradlew --offline --no-daemon tasks --all > "$report_dir/gradle-tasks.txt"
        # The Rust sources are not copied here: reuse the core and bindings from the image build.
        sh ./gradlew --offline --no-daemon --rerun-tasks -Pdyapp.prebuiltRust :app:testDebugUnitTest
        python3 /app/tests/scripts/check-junit-results.py /tmp/build/android/app/test-results/testDebugUnitTest
        ;;
    emulator)
        export ANDROID_USER_HOME=/home/appuser/.android
        mkdir -p "$ANDROID_USER_HOME"
        # /avd is a disk volume (see compose.ui.yml); --force drops the previous run's AVD.
        echo no | avdmanager create avd --force -n test_device -k 'system-images;android-31;default;x86_64' -p /avd/test_device
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
                grep -E 'mCurrentFocus|mFocusedApp' "$report_dir/window.txt" || true
            fi
            timeout 30 adb -s emulator-5554 logcat -d \
                > "$report_dir/logcat.txt" 2>&1 || true
            exit "$code"
        }
        trap collect_ui_reports EXIT
        timeout 300 adb -s emulator-5554 install --no-streaming -r /app/build/android/app/outputs/apk/debug/app-debug.apk
        timeout 300 adb -s emulator-5554 install --no-streaming -r /app/build/android/app/outputs/apk/androidTest/debug/app-debug-androidTest.apk
        # A freshly booted emulator may still show the lock screen or a system ANR dialog over
        # the app; AppUITests requires the app to own the active window.
        timeout 60 adb -s emulator-5554 shell settings put global hide_error_dialogs 1
        timeout 60 adb -s emulator-5554 shell input keyevent KEYCODE_WAKEUP
        timeout 60 adb -s emulator-5554 shell wm dismiss-keyguard
        # The broadcast waits for every receiver; a software-emulated API 31 needs minutes.
        timeout 300 adb -s emulator-5554 shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS
        # am instrument has no retries: rerun a failed suite twice; failures before a passing
        # run are flaky (practice B5).
        failed=()
        for attempt in 1 2 3; do
            timeout 900 adb -s emulator-5554 shell am instrument -w com.dyapp.test/androidx.test.runner.AndroidJUnitRunner > "$report_dir/instrumentation.txt" || true
            if python3 /app/tests/scripts/check-instrumentation-results.py "$report_dir/instrumentation.txt"; then
                break
            fi
            [[ $attempt != 3 ]] || exit 1
            cp -f "$report_dir/instrumentation.txt" "$report_dir/instrumentation-$attempt.txt"
            failed+=("$report_dir/instrumentation-$attempt.txt")
        done
        python3 /app/tests/scripts/check-flaky.py "${failed[@]}"
        timeout 60 adb -s emulator-5554 exec-out run-as com.dyapp cat files/ui-ready.png > "$report_dir/ready.png"
        test -s "$report_dir/ready.png"
        ;;
    *) echo "Unknown suite: $1" >&2; exit 2 ;;
esac
