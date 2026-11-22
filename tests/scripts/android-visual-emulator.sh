#!/bin/bash
# Rendering fixtures need an emulator, not a host node or native network peer.
set -euo pipefail
bash /runner/tests/scripts/check-container-isolation.sh
export ANDROID_USER_HOME=/home/appuser/.android
mkdir -p "$ANDROID_USER_HOME" /app/build/visual-runtime
[[ ! -e /avd/owner_visual ]] || { echo 'Use a fresh visual AVD directory' >&2; exit 1; }
echo no | avdmanager create avd -n owner_visual \
    -k 'system-images;android-31;default;x86_64' -p /avd/owner_visual
adb -a -P 5037 nodaemon server > /app/build/visual-runtime/adb.log 2>&1 &
exec emulator -avd owner_visual -port 5554 -accel on -no-window -no-audio -no-metrics \
    -no-boot-anim -no-snapshot -gpu swiftshader_indirect -memory 2048 -cores 2 -feature -Vulkan
