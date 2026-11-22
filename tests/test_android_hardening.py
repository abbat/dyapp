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


def shared_elf(
    machine: int = 62, hash_style: str = "sysv", **changes: Unpack[ElfOptions]
) -> bytes:
    options: ElfOptions = {**GOOD, "interp": False, **changes}
    body = bytearray(elf(**options))
    struct.pack_into("<H", body, 18, machine)
    phoff = struct.unpack_from("<Q", body, 32)[0]
    stride, count = struct.unpack_from("<HH", body, 54)
    for index in range(count):
        at = phoff + index * stride
        if struct.unpack_from("<I", body, at)[0] == 1:
            struct.pack_into("<Q", body, at + 48, 16384)
    return with_symbol_table(body, hash_style)


def with_symbol_table(body: bytearray, hash_style: str) -> bytes:
    """Materialize real loader-linked dynsym entries, not just name strings."""
    phoff = struct.unpack_from("<Q", body, 32)[0]
    stride, count = struct.unpack_from("<HH", body, 54)
    headers = [phoff + index * stride for index in range(count)]
    dynamic = next(
        at for at in headers if struct.unpack_from("<I", body, at)[0] == 2
    )
    start = struct.unpack_from("<Q", body, dynamic + 8)[0]
    size = struct.unpack_from("<Q", body, dynamic + 32)[0]
    tags = dict(
        struct.unpack_from("<qQ", body, at)
        for at in range(start, start + size - 16, 16)
    )
    strings = bytes(body[tags[5] : tags[5] + tags[10]])
    symbols = bytearray(24)
    name = 1
    for value in strings[1:].split(b"\x00"):
        if value:
            symbols.extend(struct.pack("<IBBHQQ", name, 0x12, 0, 0, 0, 0))
        name += len(value) + 1
    symbol_at = len(body)
    body.extend(symbols)
    hash_at = len(body)
    symbol_count = len(symbols) // 24
    if hash_style == "gnu":
        hash_data = struct.pack("<IIIIQI", 1, symbol_count, 1, 6, 0, 0)
        tag, kind = 0x6FFFFEF5, 0x6FFFFFF6
    else:
        hash_data = struct.pack("<II", 1, symbol_count) + bytes(
            4 * (1 + symbol_count)
        )
        tag, kind = 4, 5
    body.extend(hash_data)
    tags.update({6: symbol_at, 11: 24, tag: hash_at})
    dynamic_at = len(body)
    entries = b"".join(
        struct.pack("<qQ", tag, value) for tag, value in tags.items()
    )
    entries += bytes(16)
    body.extend(entries)
    section_at = len(body)
    body.extend(bytes(64))
    body.extend(
        struct.pack(
            "<IIQQQQIIQQ", 0, 3, 2, tags[5], tags[5], tags[10], 0, 0, 1, 0
        )
    )
    body.extend(
        struct.pack(
            "<IIQQQQIIQQ",
            0,
            11,
            2,
            symbol_at,
            symbol_at,
            len(symbols),
            1,
            1,
            8,
            24,
        )
    )
    body.extend(
        struct.pack(
            "<IIQQQQIIQQ",
            0,
            kind,
            2,
            hash_at,
            hash_at,
            len(hash_data),
            2,
            0,
            4,
            4,
        )
    )
    struct.pack_into("<Q", body, 40, section_at)
    struct.pack_into("<HH", body, 58, 64, 4)
    struct.pack_into("<QQ", body, dynamic + 8, dynamic_at, dynamic_at)
    struct.pack_into("<QQ", body, dynamic + 32, len(entries), len(entries))
    for at in headers:
        if struct.unpack_from("<I", body, at)[0] == 1:
            struct.pack_into("<QQ", body, at + 32, len(body), len(body))
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

    @unittest.skipUnless(os.name == "posix", "cross-UID Docker artifact reads")
    def test_failure_diagnostic_is_readable_by_the_artifact_collector(
        self,
    ) -> None:
        bad = self.run_apk({"lib/x86_64/libdyapp_ffi.so": b"invalid ELF"})
        self.assertEqual(bad.returncode, 1)
        logs = list((self.root / "logs").glob("hardening-*.log"))
        self.assertEqual(len(logs), 1)
        diagnostic = logs[0]
        self.assertEqual(diagnostic.stat().st_mode & 0o444, 0o444)
        self.assertIn("lib/x86_64/libdyapp_ffi.so", diagnostic.read_text())
        self.assertIn(str(diagnostic), bad.stdout)

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
            "canary": (
                {"names": [b"__stack_chk_fail_local", b"__memcpy_chk"]},
                "protector",
            ),
            "fortify": ({"names": [b"__stack_chk_fail"]}, "fortified libc"),
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

    def test_names_without_real_imports_cannot_prove_hardening(self) -> None:
        for symbol, message in ((1, "protector"), (2, "fortified libc")):
            with self.subTest(symbol=symbol):
                body = bytearray(shared_elf())
                sections = struct.unpack_from("<Q", body, 40)[0]
                symbols = struct.unpack_from("<Q", body, sections + 128 + 24)[
                    0
                ]
                # An exported name remains in dynstr but is not an import.
                struct.pack_into("<H", body, symbols + symbol * 24 + 6, 1)
                result = self.run_apk(
                    {"lib/x86_64/libdyapp_ffi.so": bytes(body)}
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stdout)

    def test_imported_data_cannot_stand_in_for_a_canary_call(self) -> None:
        body = bytearray(shared_elf())
        sections = struct.unpack_from("<Q", body, 40)[0]
        symbols = struct.unpack_from("<Q", body, sections + 128 + 24)[0]
        body[symbols + 24 + 4] = 0x11  # STB_GLOBAL / STT_OBJECT
        result = self.run_apk({"lib/x86_64/libdyapp_ffi.so": bytes(body)})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("protector", result.stdout)

    def test_gnu_only_inventory_accepts_undefined_symbol_prefix(self) -> None:
        result = self.run_apk(
            {"lib/x86_64/libdyapp_ffi.so": shared_elf(hash_style="gnu")}
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_invalid_import_inventory_fails_closed(self) -> None:
        for case in ("missing", "address", "entry", "link", "name", "short"):
            with self.subTest(case=case):
                body = bytearray(shared_elf())
                sections = struct.unpack_from("<Q", body, 40)[0]
                table = sections + 128
                symbols = struct.unpack_from("<Q", body, table + 24)[0]
                if case == "missing":
                    struct.pack_into("<H", body, 60, 0)
                elif case == "address":
                    struct.pack_into("<Q", body, table + 16, len(body) + 1)
                elif case == "entry":
                    struct.pack_into("<Q", body, table + 56, 0)
                elif case == "link":
                    struct.pack_into("<I", body, table + 40, 99)
                elif case == "short":
                    struct.pack_into("<Q", body, table + 32, 24)
                else:
                    struct.pack_into("<I", body, symbols + 24, len(body))
                result = self.run_apk(
                    {"lib/x86_64/libdyapp_ffi.so": bytes(body)}
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("unverifiable ELF imports", result.stdout)

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
