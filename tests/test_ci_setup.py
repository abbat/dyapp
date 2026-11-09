"""Build-only CI setup must not fetch unrelated quality or UI tools."""

import json
import os
from pathlib import Path
import subprocess
import time
import tomllib
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
STUB = """#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
name = Path(sys.argv[0]).name
with open(os.environ["DYAPP_SETUP_COMMAND_LOG"], "a") as stream:
    stream.write(json.dumps([name, *sys.argv[1:]]) + "\\n")
if name in ("docker", "curl"):
    sys.exit(94)
if name == "sudo" and sys.argv[1] != "apt-get":
    sys.exit(95)
"""


class CiSetupTest(unittest.TestCase):
    def run_setup(self, profile):
        directory = ROOT / "build/tool-fixtures/ci-setup"
        directory = directory / str(time.time_ns())
        binaries = directory / "bin"
        binaries.mkdir(parents=True)
        for name in ("sudo", "rustup", "curl", "docker"):
            path = binaries / name
            path.write_text(STUB)
            path.chmod(0o755)
        log = directory / "commands.jsonl"
        exports = directory / "github-env"
        search_path = str(binaries) + os.pathsep + os.environ["PATH"]
        env = dict(
            os.environ,
            PATH=search_path,
            GITHUB_ENV=str(exports),
            DYAPP_SETUP_COMMAND_LOG=str(log),
        )
        env.pop("BASH_ENV", None)
        result = subprocess.run(
            ["bash", str(ROOT / "tools/scripts/ci-setup.sh"), profile],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        commands = [json.loads(line) for line in log.read_text().splitlines()]
        return result, commands, exports

    def test_build_profiles_request_only_build_prerequisites(self) -> None:
        channel = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())
        channel = channel["toolchain"]["channel"]
        unrelated = {
            "python3-flake8",
            "shellcheck",
            "xvfb",
            "xauth",
            "dbus-x11",
            "gnome-keyring",
            "libsecret-tools",
        }
        for profile in ("rust-build", "linux-build"):
            with self.subTest(profile=profile):
                result, commands, exports = self.run_setup(profile)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    [row[0] for row in commands], ["sudo", "sudo", "rustup"]
                )
                packages = set(commands[1][6:])
                self.assertTrue(
                    {"build-essential", "pkg-config", "libssl-dev"} <= packages
                )
                self.assertFalse(packages & unrelated)
                needed = (
                    "protobuf-compiler"
                    if profile == "rust-build"
                    else "libwebkit2gtk-4.1-dev"
                )
                self.assertIn(needed, packages)
                expected = [
                    "rustup",
                    "toolchain",
                    "install",
                    channel,
                    "--profile",
                    "minimal",
                ]
                self.assertEqual(commands[2], expected)
                self.assertEqual(
                    exports.read_text(), f"RUSTUP_TOOLCHAIN={channel}\n"
                )

    def test_quality_profile_keeps_its_tool_downloads(self) -> None:
        result, commands, _ = self.run_setup("rust")
        self.assertEqual(result.returncode, 94)
        self.assertIn("python3-flake8", commands[1])
        self.assertEqual(commands[2][:2], ["docker", "create"])
        self.assertTrue(commands[2][2].startswith("rhysd/actionlint@sha256:"))

    def test_reproducibility_selects_both_build_profiles(self) -> None:
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/reproducible.yml").read_text()
        )
        matrix = workflow["jobs"]["compare"]["strategy"]["matrix"]["include"]
        self.assertEqual(
            {row["target"]: row["setup"] for row in matrix},
            {"dyappd": "rust-build", "linux": "linux-build"},
        )


if __name__ == "__main__":
    unittest.main()
