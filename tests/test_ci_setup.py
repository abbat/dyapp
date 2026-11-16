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
    def run_setup(
        self, profile: "str"
    ) -> "tuple[subprocess.CompletedProcess[str], list[list[str]], Path]":
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
            TMPDIR=str(directory),
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
                self.assertLessEqual(
                    {"build-essential", "pkg-config", "libssl-dev"}, packages
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

    def test_failed_download_names_the_public_url_and_preserves_failure(
        self,
    ) -> None:
        result, commands, _ = self.run_setup("linux")
        self.assertEqual(result.returncode, 94)
        url = "https://registry.npmjs.org/axe-core/-/axe-core-4.14.0.tgz"
        self.assertEqual(commands[-1][:3], ["curl", "--retry", "7"])
        self.assertEqual(commands[-1][-1], url)
        self.assertIn(
            f"Pinned download failed (curl exit 94): {url}", result.stderr
        )
        self.assertFalse(any(row[0] == "rustup" for row in commands))

    def test_fuzz_jobs_use_only_build_prerequisites(self) -> None:
        for name in ("fuzz.yml", "ci-base.yml"):
            with self.subTest(workflow=name):
                workflow = yaml.safe_load(
                    (ROOT / ".github/workflows" / name).read_text()
                )
                steps = workflow["jobs"]["fuzz"]["steps"]
                setup = next(
                    step for step in steps if step.get("name") == "setup"
                )
                self.assertEqual(
                    setup["run"], "bash tools/scripts/ci-setup.sh rust-build"
                )
                self.assertTrue(
                    any(
                        step.get("with", {}).get("tool") == "cargo-fuzz@0.13.2"
                        for step in steps
                    )
                )

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
