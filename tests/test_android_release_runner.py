"""Exercise unsigned-package export gates without Gradle or an SDK install."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile

from test_android_hardening import shared_elf

ROOT = Path(__file__).resolve().parents[1]


class AndroidReleaseRunner(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / "build")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.scripts = self.root / "tools/scripts"
        shutil.copytree(
            ROOT / "tools/scripts",
            self.scripts,
            ignore=shutil.ignore_patterns("__pycache__"),
        )
        (self.root / "clients/android").mkdir(parents=True)
        (self.scripts / "android_release_environment.py").write_text(
            "# Prepared package fixture: no Gradle or compiler is run.\n"
        )
        self.reports = self.root / "reports"
        self.receipt = self.root / "zipalign-args.json"
        self.sdk = self.root / "sdk"
        self.tool = self.sdk / "build-tools/36.0.0/zipalign"
        self.tool.parent.mkdir(parents=True)
        self.tool.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, sys\n"
            "receipt = pathlib.Path(os.environ['ALIGN_RECEIPT'])\n"
            "receipt.write_text(json.dumps(sys.argv[1:]))\n"
            "sys.exit(int(os.environ['ALIGN_STATUS']))\n"
        )
        self.tool.chmod(0o755)

    def package(self, kind: str, bad_jni: bool = False) -> Path:
        suffix = (
            "apk/release/app-release-unsigned.apk"
            if kind == "apk"
            else "bundle/release/app-release.aab"
        )
        path = self.root / "build/android/app/outputs" / suffix
        path.parent.mkdir(parents=True, exist_ok=True)
        prefix = "" if kind == "apk" else "base/"
        manifest = (
            "AndroidManifest.xml"
            if kind == "apk"
            else "base/manifest/AndroidManifest.xml"
        )
        dex = "classes.dex" if kind == "apk" else "base/dex/classes.dex"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(manifest, b"manifest")
            archive.writestr(dex, b"dex")
            for abi, machine in (("arm64-v8a", 183), ("x86_64", 62)):
                archive.writestr(
                    f"{prefix}lib/{abi}/libdyapp_ffi.so",
                    shared_elf(machine=machine),
                )
            archive.writestr(
                f"{prefix}lib/x86_64/libjnidispatch.so",
                shared_elf(
                    names=[b"__memcpy_chk"]
                    if bad_jni
                    else [b"__stack_chk_fail", b"__memcpy_chk"]
                ),
            )
        return path

    def run_release(
        self, alignment_status: int = 0
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "bash",
                str(self.scripts / "build-android-release.sh"),
                "--raw",
                str(self.reports),
            ],
            env={
                **os.environ,
                "ANDROID_SDK_ROOT": str(self.sdk),
                "DYAPP_CHECK_LOG_DIR": str(self.reports / "check-output"),
                "ALIGN_RECEIPT": str(self.receipt),
                "ALIGN_STATUS": str(alignment_status),
            },
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )

    def require_no_export(self) -> None:
        self.assertFalse((self.reports / "app-release-unsigned.apk").exists())
        self.assertFalse((self.reports / "app-release.aab").exists())

    def test_verified_packages_export_only_after_alignment(self) -> None:
        apk = self.package("apk")
        aab = self.package("aab")
        result = self.run_release()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            json.loads(self.receipt.read_text()),
            ["-c", "-P", "16", "4", str(apk)],
        )
        self.assertEqual(
            (self.reports / apk.name).read_bytes(), apk.read_bytes()
        )
        self.assertEqual(
            (self.reports / aab.name).read_bytes(), aab.read_bytes()
        )

    def test_noncompliant_dependency_never_reaches_alignment_or_export(
        self,
    ) -> None:
        self.package("apk", bad_jni=True)
        self.package("aab")
        result = self.run_release()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("libjnidispatch.so", result.stdout + result.stderr)
        self.assertFalse(self.receipt.exists())
        self.require_no_export()

    def test_alignment_failure_prevents_both_exports(self) -> None:
        self.package("apk")
        self.package("aab")
        result = self.run_release(alignment_status=7)
        self.assertEqual(result.returncode, 7, result.stdout + result.stderr)
        self.assertTrue(self.receipt.exists())
        self.require_no_export()

    def test_nonexecutable_alignment_tool_cannot_export(self) -> None:
        self.package("apk")
        self.package("aab")
        self.tool.chmod(0o644)
        result = self.run_release()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.receipt.exists())
        self.require_no_export()


if __name__ == "__main__":
    unittest.main()
