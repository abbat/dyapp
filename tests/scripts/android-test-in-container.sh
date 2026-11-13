#!/bin/bash
set -euo pipefail
bash /app/tests/scripts/check-container-isolation.sh
mkdir -p /tmp/ai
export DYAPP_CHECK_LOG_DIR="/reports/$HOSTNAME/check-output"
case "${1:?suite required}" in
    build)
        # The checkout is read-only and Gradle writes into its project: build a copy whose build/
        # is the mounted /app/build, so the outputs and caches stay there between runs.
        mkdir -p /tmp/ai/src
        cp -a /app/Cargo.toml /app/Cargo.lock /app/.cargo /app/proto /app/rust /app/clients /tmp/ai/src/
        cp -f /app/detekt.yml /app/detekt-baseline.xml /app/.editorconfig /tmp/ai/src/
        ln -s /app/build /tmp/ai/src/build
        cd /tmp/ai/src
        echo '3b8a25775a69158b5ad2b1d17a80a88dc7b40a352a73eb27d06c612a7ce68e98  clients/android/gradle/wrapper/gradle-wrapper.jar' | sha256sum -c -
        # The debug core is x86_64 only: the emulator is the only device these packages run on.
        (cd clients/android && ./gradlew --no-daemon -Pdyapp.debugAbis=x86_64 \
            :app:assembleDebug :app:assembleDebugAndroidTest :app:compileDebugUnitTestKotlin :app:lintDebug)
        # Host dyappd for the emulator runner's loopback node.
        cargo build --locked -p dyapp-bootstrap --bin dyappd
        ;;
    unit)
        mkdir -p /tmp/ai/src/clients /tmp/build/android
        cp -rf /app/clients/android /tmp/ai/src/clients/android
        cp -f /app/detekt.yml /app/detekt-baseline.xml /app/.editorconfig /tmp/ai/src/
        ln -s /tmp/build /tmp/ai/src/build
        cp -rf /app/build/android/app /tmp/build/android/app
        cd /tmp/ai/src/clients/android
        report_dir="/reports/$HOSTNAME"
        mkdir -p "$report_dir"
        save_reports() {
            local code=$? crash
            for crash in /tmp/ai/src/clients/android/hs_err*.log; do
                if [[ -f $crash ]]; then head -60 "$crash"; cp -f "$crash" "$report_dir/"; fi
            done
            cp -rf /tmp/build/android/app/test-results "$report_dir/" 2>/dev/null || true
            exit "$code"
        }
        trap save_reports EXIT
        sh ./gradlew --offline --no-daemon --version > "$report_dir/gradle-version.txt"
        cat "$report_dir/gradle-version.txt"
        # No `tasks --all`: realising every task fails on AGP 9.5's release Compose Preview task.
        # The Rust sources are not copied here: reuse the core and bindings of the build suite.
        sh ./gradlew --offline --no-daemon --rerun-tasks -Pdyapp.prebuiltRust :app:testDebugUnitTest
        python3 /app/tests/scripts/check-junit-results.py /tmp/build/android/app/test-results/testDebugUnitTest
        ;;
    emulator)
        export ANDROID_USER_HOME=/home/appuser/.android
        mkdir -p "$ANDROID_USER_HOME"
        # /avd is a disk volume (see compose.ui.yml); --force drops the previous run's AVD.
        echo no | avdmanager create avd --force -n test_device -k 'system-images;android-31;default;x86_64' -p /avd/test_device
        adb -a -P 5037 nodaemon server > /tmp/ai/adb.log 2>&1 &
        # Loopback dyappd for the publish test; the guest reaches this container's 127.0.0.1 at 10.0.2.2.
        # Media serves the profile photos a publish uploads; without it the node replies Unsupported.
        export DYAPPD_LISTEN='["127.0.0.1:33445"]' DYAPPD_EXTERNAL='["10.0.2.2:33445"]' \
            DYAPPD_STORAGE_DIR=/reports/node/storage DYAPPD_NETWORK_ID_POW_BITS=8 \
            DYAPPD_SEARCH_MIN_PROFILE_POW_BITS=4 \
            DYAPPD_ROLES='["store", "search", "media"]'
        # The UI container forges this index after the suite, so it lives on the shared volume.
        rm -rf /reports/node && mkdir -p /reports/node
        /app/build/debug/dyappd keygen > /reports/node/keygen.log 2>&1
        control=/app/tests/scripts/android-node-control.sh
        bash "$control" init /reports/node
        bash "$control" serve /reports/node /app/build/debug/dyappd &
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
        # The keyboard opening after typed text moves the layout under the next tap. Compose tests
        # type through semantics, so they need no keyboard.
        timeout 60 adb -s emulator-5554 shell ime disable com.android.inputmethod.latin/.LatinIME
        # The broadcast waits for every receiver; a software-emulated API 31 needs minutes.
        timeout 300 adb -s emulator-5554 shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS
        # am instrument has no retries: rerun the failed tests twice; failures before a passing
        # run are flaky (practice B5). A failed run that names no test reruns the whole suite.
        failed=() only=() minimum=3
        for attempt in 1 2 3; do
            timeout 900 adb -s emulator-5554 shell am instrument -w "${only[@]}" com.dyapp.test/androidx.test.runner.AndroidJUnitRunner > "$report_dir/instrumentation.txt" || true
            if python3 /app/tests/scripts/check-instrumentation-results.py "$report_dir/instrumentation.txt" "$minimum"; then
                break
            fi
            [[ $attempt != 3 ]] || exit 1
            cp -f "$report_dir/instrumentation.txt" "$report_dir/instrumentation-$attempt.txt"
            failed+=("$report_dir/instrumentation-$attempt.txt")
            # The same pattern as check-flaky.py; adb shell passes the list to a device shell,
            # so it is single-quoted for nested classes ($).
            tests=$(sed -nE 's/^[0-9]+\) ([A-Za-z0-9_]+)\(([A-Za-z0-9_.$]+)\)$/\2#\1/p' \
                "$report_dir/instrumentation-$attempt.txt" | paste -sd, -)
            listed=$(grep -cE '^[0-9]+\) ' "$report_dir/instrumentation-$attempt.txt" || true)
            if [[ -n $tests && $(tr , '\n' <<< "$tests" | wc -l) == "$listed" ]]; then
                only=(-e class "'$tests'") minimum=1
            else
                only=() minimum=3
            fi
        done
        python3 /app/tests/scripts/check-flaky.py "${failed[@]}"
        # Separate invocations exercise isolated and actual-worker native registries.
        peer_phase() {
            local fixture=$1 phase=$2 argument test_class deadline=300
            case "$fixture:$phase" in
                worker:prepare|worker:catchUpScheduled) deadline=900 ;;
            esac
            case "$fixture" in
                retained) argument=retainedPeerPhase; test_class=RetainedPeerPhaseTests ;;
                worker) argument=workerPeerPhase; test_class=WorkerPeerPhaseTests ;;
                *) return 2 ;;
            esac
            timeout "$deadline" adb -s emulator-5554 shell am instrument -w \
                -e "$argument" "$phase" -e class "com.dyapp.$test_class" \
                com.dyapp.test/androidx.test.runner.AndroidJUnitRunner \
                > "$report_dir/$fixture-peer-$phase.txt" || return
            python3 /app/tests/scripts/check-instrumentation-results.py "$report_dir/$fixture-peer-$phase.txt" 1
        }
        node_action() {
            timeout 30 bash /app/tests/scripts/android-node-control.sh command /reports/node "$1" \
                > "$report_dir/node-$1.txt"
        }
        offline_worker_phases() {
            node_action stop || return
            if peer_phase worker queue && peer_phase worker reopenQueued; then
                node_action start || return
                peer_phase worker catchUpScheduled
            else
                node_action start || true
                return 1
            fi
        }
        for fixture in retained worker; do
            if peer_phase "$fixture" prepare && peer_phase "$fixture" reopen; then
                if [[ $fixture == worker ]] && ! offline_worker_phases; then
                    peer_phase worker cleanup || true
                    exit 1
                fi
                peer_phase "$fixture" cleanup
            else
                peer_phase "$fixture" cleanup || true
                exit 1
            fi
        done
        # Forged-result fixture: with every indexed signature flipped, a search must list no profile.
        python3 /app/tests/scripts/forge-search-index.py /reports/node/storage
        timeout 300 adb -s emulator-5554 shell am instrument -w -e forged 1 \
            -e class com.dyapp.SearchTests#forgedResultsAreDroppedAndListed \
            com.dyapp.test/androidx.test.runner.AndroidJUnitRunner > "$report_dir/forged.txt" || true
        cat "$report_dir/forged.txt"
        grep -q '^OK (1 test)' "$report_dir/forged.txt"
        timeout 60 adb -s emulator-5554 exec-out run-as com.dyapp cat files/ui-ready.png > "$report_dir/ready.png"
        test -s "$report_dir/ready.png"
        ;;
    *) echo "Unknown suite: $1" >&2; exit 2 ;;
esac
