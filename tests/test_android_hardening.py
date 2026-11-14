"""The shipped APK must meet the shared ELF policy for every native library."""

import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile
from typing import Unpack

from test_hardening_output import elf, ElfOptions, GOOD

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools/scripts/check-hardening.py"


def shared_elf(machine: int = 62, **changes: Unpack[ElfOptions]) -> bytes:
    options: ElfOptions = {**GOOD, "interp": False, **changes}
    body = bytearray(elf(**options))
    struct.pack_into("<H", body, 18, machine)
    phoff = struct.unpack_from("<Q", body, 32)[0]
    stride, count = struct.unpack_from("<HH", body, 54)
    for index in range(count):
        at = phoff + index * stride
        if struct.unpack_from("<I", body, at)[0] == 1:
            struct.pack_into("<Q", body, at + 48, 16384)
    return bytes(body)


class AndroidHardening(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / "build")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def run_apk(
        self, changes: dict[str, bytes | None], raw: bool = False
    ) -> subprocess.CompletedProcess[str]:
        entries: dict[str, bytes | None] = {
            "lib/arm64-v8a/libdyapp_ffi.so": shared_elf(machine=183),
            "lib/x86_64/libdyapp_ffi.so": shared_elf(),
            **changes,
        }
        apk = self.root / "signed.apk"
        with zipfile.ZipFile(apk, "w") as archive:
            for name, body in entries.items():
                if body is not None:
                    archive.writestr(name, body)
        return self.check(apk, raw)

    def check(
        self, apk: Path, raw: bool = False
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, "-B", str(SCRIPT)]
            + (["--raw"] if raw else [])
            + ["--android-apk", str(apk)],
            capture_output=True,
            text=True,
            check=False,
            env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(self.root / "logs")),
        )

    def test_both_abis_and_third_party_jni_are_checked(self) -> None:
        good = self.run_apk({"lib/x86_64/libjnidispatch.so": shared_elf()})
        self.assertEqual(good.returncode, 0, good.stderr + good.stdout)
        self.assertEqual(good.stdout, "OK hardening\n")
        bad = self.run_apk(
            {"lib/x86_64/libjnidispatch.so": shared_elf(stack=7)}
        )
        self.assertEqual(bad.returncode, 1)
        self.assertIn("libjnidispatch.so: executable stack", bad.stdout)
        self.assertIn("raw captured output", bad.stdout)

    def test_shared_profile_rejects_each_protection_regression(self) -> None:
        changes: dict[str, tuple[ElfOptions, str]] = {
            "executable": ({"e_type": 2}, "ET_DYN shared library"),
            "interpreter": ({"interp": True}, "ET_DYN shared library"),
            "stack": ({"stack": None}, "executable stack"),
            "relro": ({"relro": False}, "no RELRO segment"),
            "lazy": ({"dynamic": {}}, "RELRO is partial"),
            "rpath": ({"dynamic": {30: 8, 15: 1}}, "RPATH or RUNPATH"),
            "textrel": ({"dynamic": {30: 8, 22: 0}}, "text relocations"),
            "flags-textrel": ({"dynamic": {30: 12}}, "text relocations"),
            "canary": ({"names": [b"__stack_chk_fail_local"]}, "protector"),
        }
        for name, (change, message) in changes.items():
            with self.subTest(name=name):
                result = self.run_apk(
                    {"lib/x86_64/libdyapp_ffi.so": shared_elf(**change)},
                    raw=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)

    def test_load_alignment_and_permissions_are_not_inferred_from_ndk(
        self,
    ) -> None:
        for field, value, message in (
            (48, 4096, "not 16 KiB aligned"),
            (48, 24576, "not 16 KiB aligned"),
            (16, 4096, "not 16 KiB aligned"),
            (4, 7, "writable executable PT_LOAD"),
        ):
            with self.subTest(field=field, value=value):
                body = bytearray(shared_elf())
                struct.pack_into(
                    "<I" if field == 4 else "<Q", body, 64 + field, value
                )
                result = self.run_apk(
                    {"lib/x86_64/libdyapp_ffi.so": bytes(body)}
                )
                self.assertEqual(result.returncode, 1)
                self.assertIn(message, result.stdout)

    def test_missing_wrong_and_truncated_payloads_fail_closed(self) -> None:
        for body, message in (
            (None, "missing core library"),
            (shared_elf(machine=183), "wrong ELF architecture"),
            (b"\x7fELF", "truncated ELF"),
            (shared_elf()[:100], "invalid ELF program headers"),
        ):
            with self.subTest(message=message):
                result = self.run_apk({"lib/x86_64/libdyapp_ffi.so": body})
                self.assertEqual(result.returncode, 1)
                self.assertIn(message, result.stdout)

    def test_duplicate_entries_and_unsupported_abis_fail(self) -> None:
        bad = self.run_apk({"lib/armeabi-v7a/libextra.so": shared_elf()})
        self.assertEqual(bad.returncode, 1)
        self.assertIn("unsupported native ABI", bad.stdout)
        apk = self.root / "signed.apk"
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(apk, "a") as archive:
                archive.writestr("lib/x86_64/libdyapp_ffi.so", shared_elf())
        result = self.check(apk)
        self.assertEqual(result.returncode, 1)
        self.assertIn("duplicate APK entries", result.stdout)


if __name__ == "__main__":
    unittest.main()
