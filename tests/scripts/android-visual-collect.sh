#!/bin/bash
# Complete the remote archive before transferring binary bytes through exec-out.
set -euo pipefail
report=${1:?report directory}
directory=${2:?remote capture directory}
name=${3:?archive name}
[[ $name == native || $name == ime || $name == receive || $name == comparison || $name == views || $name == updates || $name == activity || $name == recovery ]]
[[ $directory =~ ^files/[a-z-]+$ ]]
diagnostic="$report/$name-collection-error.txt"
remote="files/native-visual-export-$name.tar"
if timeout 60 adb -s emulator-5554 shell run-as com.dyapp \
    tar -cf "$remote" "$directory" > "$diagnostic" 2>&1; then
    timeout 60 adb -s emulator-5554 shell run-as com.dyapp \
        sha256sum "$remote" > "$report/$name-archive.sha256" 2>> "$diagnostic"
    read -r expected remote_name < "$report/$name-archive.sha256"
    remote_name=${remote_name%$'\r'}
    if [[ ! $expected =~ ^[0-9a-fA-F]{64}$ || $remote_name != "$remote" ]]; then
        printf 'Invalid remote checksum for visual archive %s\n' "$name" | tee -a "$diagnostic" >&2
        exit 1
    fi
    # exec-out has no remote exit status; verify exact bytes, including any trailing diagnostic.
    timeout 60 adb -s emulator-5554 exec-out run-as com.dyapp \
        cat "$remote" > "$report/$name.tar" 2>> "$diagnostic"
    actual=$(sha256sum -- "$report/$name.tar")
    if [[ ${actual%% *} != "${expected,,}" ]]; then
        printf 'Visual archive %s transfer checksum mismatch\n' "$name" | tee -a "$diagnostic" >&2
        exit 1
    fi
else
    echo "Visual archive $name was not created; see $diagnostic" >&2
    exit 1
fi
