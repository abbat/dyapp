"""Inspect Apple core command plans using fake native tools in Docker."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
FAKE = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
args = sys.argv[1:]
tool = Path(sys.argv[0]).name
with open(os.environ["DYAPP_APPLE_TEST_LOG"], "a") as log:
    log.write(json.dumps([tool, *args]) + "\n")
if tool == "rustc":
    print("host: aarch64-apple-darwin")
elif tool == "cargo":
    target = args[args.index("--target") + 1]
    profile = "release" if "--release" in args else "debug"
    folder = Path("target") / target / profile
    folder.mkdir(parents=True, exist_ok=True)
    if args[0] == "build":
        bindgen = folder / "uniffi-bindgen"
        bindgen.write_text(
            "#!/bin/sh\n"
            "while [ \"$1\" != \"--out-dir\" ]; do shift; done\n"
            "shift\nmkdir -p \"$1\"\n"
            "touch \"$1/dyappFFI.h\" \"$1/dyappFFI.modulemap\" "
            "\"$1/dyapp.swift\"\n")
        bindgen.chmod(0o755)
    else:
        (folder / "libdyapp_ffi.a").write_bytes(b"fixture")
elif tool == "lipo":
    Path(args[args.index("-output") + 1]).write_bytes(b"fixture")
elif tool == "xcodebuild":
    Path(args[args.index("-output") + 1]).mkdir()
'''


class AppleCoreProfiles(unittest.TestCase):
    def test_release_device_and_default_simulator_command_plans(self):
        cases = [("ios-device", "release", 1), ("ios", "debug", 3),
                 ("macos", "release", 2)]
        for scope, profile, count in cases:
            with self.subTest(scope=scope, profile=profile):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    scripts = root / "scripts"
                    scripts.mkdir()
                    script = scripts / "build-apple-core.sh"
                    original = ROOT / "scripts/build-apple-core.sh"
                    shutil.copyfile(original, script)
                    tools = root / "fake-tools"
                    tools.mkdir()
                    for name in ("cargo", "rustc", "lipo", "xcodebuild"):
                        tool = tools / name
                        tool.write_text(FAKE)
                        tool.chmod(0o755)
                    log = root / "calls.jsonl"
                    env = dict(os.environ, DYAPP_APPLE_TEST_LOG=str(log))
                    env["PATH"] = str(tools) + os.pathsep + env["PATH"]
                    arguments = ["bash", str(script), scope]
                    if profile == "release":
                        arguments.append(profile)
                    subprocess.run(arguments, env=env, check=True,
                                   stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE)
                    calls = [json.loads(row)
                             for row in log.read_text().splitlines()]
                    builds = [row for row in calls
                              if row[:2] == ["cargo", "rustc"]]
                    self.assertEqual(len(builds), count)
                    for row in builds:
                        self.assertEqual("--release" in row,
                                         profile == "release")
                    for row in calls:
                        if row[0] == "lipo":
                            sources = row[2:row.index("-output")]
                            self.assertTrue(all(
                                f"/{profile}/" in item for item in sources))


if __name__ == "__main__":
    unittest.main()
