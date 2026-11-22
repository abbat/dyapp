#!/bin/bash
# Complete the remote archive before transferring binary bytes through exec-out.
set -euo pipefail
report=${1:?report directory}
directory=${2:?remote capture directory}
name=${3:?archive name}
[[ $name == native || $name == ime || $name == receive ]]
[[ $directory =~ ^files/[a-z-]+$ ]]
diagnostic="$report/$name-collection-error.txt"
remote="files/native-visual-export-$name.tar"
if timeout 60 adb -s emulator-5554 shell run-as com.dyapp \
    tar -cf "$remote" "$directory" > "$diagnostic" 2>&1; then
    timeout 60 adb -s emulator-5554 exec-out run-as com.dyapp \
        cat "$remote" > "$report/$name.tar" 2>> "$diagnostic"
else
    echo "Visual archive $name was not created; see $diagnostic" >&2
    exit 1
fi
