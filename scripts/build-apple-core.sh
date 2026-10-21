#!/bin/bash
# Builds rust/ffi into build/apple/DyappFFI.xcframework and generates its Swift bindings into
# build/apple/swift; both are build outputs. Needs macOS with Xcode and the Rust targets below.
# ios includes simulators; ios-device omits them. Optional second argument: debug|release.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ${1:-} != --raw ]]; then
    exec python3 scripts/run-check.py \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        apple-core -- bash "$PWD/scripts/build-apple-core.sh" --raw "$@"
fi
shift
out=build/apple
# An unoptimised core with debuginfo is hundreds of MB per target.
export CARGO_PROFILE_DEV_DEBUG=0 IPHONEOS_DEPLOYMENT_TARGET=14.0 MACOSX_DEPLOYMENT_TARGET=11.0
case ${1:-all} in
    ios) slices=(ios ios-simulator) ;;
    ios-device) slices=(ios) ;;
    macos) slices=(macos) ;;
    all) slices=(ios ios-simulator macos) ;;
    *) echo "usage: $0 [ios|ios-device|macos|all] [debug|release]" >&2; exit 2 ;;
esac
profile=${2:-debug}
case $profile in
    debug) cargo_profile=(--profile dev) ;;
    release) cargo_profile=(--release) ;;
    *) echo "unknown Apple core profile: $profile" >&2; exit 2 ;;
esac
# One slice per platform; lipo merges the architectures of one platform. No associative arrays:
# macOS ships bash 3.2.
targets() {
    case $1 in
        ios) echo aarch64-apple-ios ;;
        ios-simulator) echo aarch64-apple-ios-sim x86_64-apple-ios ;;
        macos) echo aarch64-apple-darwin x86_64-apple-darwin ;;
    esac
}

# Library mode reads the interface from a host build of the library, as on Android.
host=$(rustc -vV | sed -n 's/^host: //p')
cargo build --locked -p dyapp-ffi --target "$host"
rm -rf "$out"
mkdir -p "$out/headers"
"build/$host/debug/uniffi-bindgen" generate --library "build/$host/debug/libdyapp_ffi.dylib" \
    --language swift --out-dir "$out/swift"
mv -f "$out/swift/dyappFFI.h" "$out/headers/"
mv -f "$out/swift/dyappFFI.modulemap" "$out/headers/module.modulemap"

args=()
for slice in "${slices[@]}"; do
    libs=()
    for target in $(targets "$slice"); do
        cargo rustc "${cargo_profile[@]}" --locked -p dyapp-ffi --lib --crate-type staticlib --target "$target"
        libs+=("build/$target/$profile/libdyapp_ffi.a")
    done
    mkdir -p "$out/$slice"
    lipo -create "${libs[@]}" -output "$out/$slice/libdyapp_ffi.a"
    args+=(-library "$out/$slice/libdyapp_ffi.a" -headers "$out/headers")
done
xcodebuild -create-xcframework "${args[@]}" -output "$out/DyappFFI.xcframework"
