"""Real subprocess controls for desktop cases and prepared unit inventories."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class DesktopOutput(unittest.TestCase):
    def test_ui_rejection_passes_and_only_failed_case_output_is_shown(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "app"
            app.write_text(
                "#!/usr/bin/env python3\n"
                "import os,sys\n"
                "if '--break-ui' in sys.argv:\n"
                " print('failed-case-detail')\n"
                " if os.environ.get('DYAPP_FIXTURE_BROKEN') == '1':\n"
                "  raise SystemExit(0)\n"
                " print('UI_SMOKE_FAIL'); raise SystemExit(1)\n"
                "print('passing-case-noise'); print('UI_SMOKE_PASS')\n")
            app.chmod(0o755)
            for broken in ("0", "1"):
                reports = root / broken
                result = subprocess.run(
                    [sys.executable, str(ROOT / "scripts/desktop-smoke.py"),
                     str(app)], capture_output=True, text=True, check=False,
                    env=dict(os.environ, RUNNER_TEMP=str(reports),
                             DYAPP_CHECK_LOG_DIR=str(root / "logs"),
                             DYAPP_XVFB_DIAGNOSTICS="1",
                             DYAPP_FIXTURE_BROKEN=broken))
                if broken == "0":
                    self.assertEqual(result.returncode, 0, result.stdout)
                    self.assertEqual(result.stdout, "OK desktop-smoke\n")
                    data = json.loads(
                        (reports / "desktop-ui-results.json").read_text())
                    self.assertEqual(len(data), 3)
                    self.assertTrue(all(case["passed"] for case in data))
                else:
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("FAIL desktop-smoke:", result.stdout)
                    self.assertIn("failed-case-detail", result.stdout)
                    self.assertNotIn("passing-case-noise", result.stdout)
                    pointer = next(line for line in result.stdout.splitlines()
                                   if line.endswith(": raw captured output"))
                    retained = Path(pointer.removesuffix(
                        ": raw captured output")).read_text()
                    self.assertIn("passing-case-noise", retained)
                    self.assertIn("failed-case-detail", retained)

    def test_rust_inventory_preserves_exit_101_and_assertion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, binary = root / "fixture.rs", root / "fixture"
            source.write_text("#[test] fn fails() { assert_eq!(1, 2); }\n")
            subprocess.run(
                ["rustc", "--test", str(source), "-o", str(binary)],
                check=True, capture_output=True)
            inventory = root / "units.json"
            inventory.write_text(json.dumps({
                "reason": "compiler-artifact", "profile": {"test": True},
                "executable": str(binary)}) + "\n")
            result = subprocess.run(
                [sys.executable,
                 str(ROOT / "scripts/run-cargo-unit-binaries.py"),
                 str(inventory)], capture_output=True, text=True, check=False,
                env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs")))
            self.assertEqual(result.returncode, 101, result.stdout)
            self.assertTrue(result.stdout.startswith("FAIL rust-test:"))
            self.assertIn("left: 1", result.stdout)
            listing = root / "bad-listing"
            listing.write_text(
                "#!/bin/sh\necho listing-failure >&2\nexit 37\n")
            listing.chmod(0o755)
            inventory.write_text(json.dumps({
                "reason": "compiler-artifact", "profile": {"test": True},
                "executable": str(listing)}) + "\n")
            bad_list = subprocess.run(
                [sys.executable,
                 str(ROOT / "scripts/run-cargo-unit-binaries.py"),
                 str(inventory)], capture_output=True, text=True, check=False,
                env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs")))
            self.assertEqual(bad_list.returncode, 37)
            self.assertTrue(any("listing-failure" in path.read_text()
                                for path in (root / "logs").glob("*.log")))
            inventory.write_text("")
            empty = subprocess.run(
                [sys.executable,
                 str(ROOT / "scripts/run-cargo-unit-binaries.py"),
                 str(inventory)], capture_output=True, text=True, check=False,
                env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs")))
            self.assertNotEqual(empty.returncode, 0)
            self.assertIn("No unit test binaries", empty.stdout)

    def test_display_failure_keeps_diagnostics_without_secrets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "app"
            app.write_text(
                "#!/usr/bin/env python3\n"
                "import sys\n"
                "if '--break-ui' in sys.argv:\n"
                " print('cannot open display :99', file=sys.stderr)\n"
                " raise SystemExit(1)\n"
                "print('UI_SMOKE_PASS')\n")
            app.chmod(0o755)
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/desktop-smoke.py"),
                 str(app)], capture_output=True, text=True, check=False,
                env=dict(os.environ, RUNNER_TEMP=str(root),
                         DYAPP_CHECK_LOG_DIR=str(root / "logs"),
                         DYAPP_XVFB_DIAGNOSTICS="1", DISPLAY=":99",
                         XAUTHORITY="secret-cookie-path",
                         DBUS_SESSION_BUS_ADDRESS="secret-session-address",
                         PRIVATE_TOKEN="secret-token"))
            self.assertEqual(result.returncode, 1, result.stdout)
            self.assertIn("cannot open display :99", result.stdout)
            logs = "\n".join(p.read_text()
                             for p in (root / "logs").glob("*.log"))
            self.assertIn("Xvfb/display diagnostics", logs)
            self.assertIn("DISPLAY=:99", logs)
            self.assertIn("X11 socket exists=", logs)
            self.assertIn("XAUTHORITY configured=True", logs)
            self.assertNotIn("secret-", logs)

    def test_xvfb_startup_error_reaches_linux_runner_and_still_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            (scripts / "linux-test.sh").write_text(
                (ROOT / "scripts/linux-test.sh").read_text())
            (scripts / "run-cargo-unit-binaries.py").write_text("")
            target = root / "clients/linux/target"
            target.mkdir(parents=True)
            (target / "unit-build.json").write_text("")
            xvfb = root / "xvfb-run"
            xvfb.write_text(
                "#!/bin/sh\n"
                "case \" $* \" in\n"
                " *' -e /dev/stderr '*) echo 'Xvfb server failed' >&2;;\n"
                " *) exit 99;;\n"
                "esac\nexit 37\n")
            xvfb.chmod(0o755)
            result = subprocess.run(
                ["bash", str(scripts / "linux-test.sh"), "--raw"],
                capture_output=True, text=True, check=False,
                env=dict(os.environ, PATH=f"{root}:{os.environ['PATH']}"))
            self.assertEqual(result.returncode, 37, result.stdout)
            self.assertIn("Xvfb server failed", result.stderr)


if __name__ == "__main__":
    unittest.main()
