"""JNA repacking preserves Java and rejects invalid native/archive inputs."""

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile

from test_android_hardening import shared_elf

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools/scripts/jna_aar.py"
SPEC = importlib.util.spec_from_file_location("jna_aar", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class JnaAar(unittest.TestCase):
    def setUp(self) -> None:
        build = ROOT / "build/jna-aar-tests"
        build.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=build)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.native = self.root / "native"
        for abi, machine in CHECK.ABIS.items():
            directory = self.native / abi
            directory.mkdir(parents=True)
            (directory / CHECK.LIBRARY).write_bytes(shared_elf(machine))
        self.entries = {
            "classes.jar": b"unchanged Java archive",
            "AndroidManifest.xml": b"unchanged Android manifest",
            "res/": b"",
            "res/raw/license": b"unchanged resource",
            "jni/x86/libjnidispatch.so": b"unsupported upstream library",
            **{
                f"jni/{abi}/{CHECK.LIBRARY}": b"upstream" for abi in CHECK.ABIS
            },
        }
        self.source = self.root / "original.aar"
        self.output = self.root / "hardened.aar"
        self.fixture(self.entries)

    def fixture(self, entries: dict[str, bytes]) -> None:
        with zipfile.ZipFile(self.source, "w") as archive:
            for name, data in entries.items():
                archive.writestr(name, data)

    def test_preserves_resources_and_packs_only_protected_abis(self) -> None:
        report = CHECK.rewrite(self.source, self.native, self.output)
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(archive.namelist(), sorted(archive.namelist()))
            for name, data in self.entries.items():
                if not name.startswith("jni/"):
                    self.assertEqual(archive.read(name), data)
            expected = {f"jni/{abi}/{CHECK.LIBRARY}" for abi in CHECK.ABIS}
            self.assertEqual(
                {
                    name
                    for name in archive.namelist()
                    if name.startswith("jni/")
                },
                expected,
            )
            for abi in CHECK.ABIS:
                self.assertEqual(
                    archive.read(f"jni/{abi}/{CHECK.LIBRARY}"),
                    (self.native / abi / CHECK.LIBRARY).read_bytes(),
                )
            for info in archive.infolist():
                self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
        self.assertEqual(
            report["classes_sha256"],
            hashlib.sha256(self.entries["classes.jar"]).hexdigest(),
        )
        self.assertEqual(
            report["sha256"],
            hashlib.sha256(self.output.read_bytes()).hexdigest(),
        )

    def test_source_order_and_metadata_do_not_change_output_bytes(
        self,
    ) -> None:
        CHECK.rewrite(self.source, self.native, self.output)
        with zipfile.ZipFile(self.source, "w") as archive:
            archive.comment = b"different upstream ZIP metadata"
            for name, data in reversed(list(self.entries.items())):
                archive.writestr(
                    zipfile.ZipInfo(name, (2001, 2, 3, 4, 5, 6)), data
                )
        other = self.root / "second.aar"
        CHECK.rewrite(self.source, self.native, other)
        self.assertEqual(self.output.read_bytes(), other.read_bytes())

    def test_bad_native_never_creates_output(self) -> None:
        changes = [
            shared_elf(machine=183),
            shared_elf(names=[b"memcpy"]),
            b"truncated ELF",
        ]
        target = self.native / "x86_64" / CHECK.LIBRARY
        for body in changes:
            with self.subTest(body_sha=hashlib.sha256(body).hexdigest()):
                target.write_bytes(body)
                with self.assertRaises(ValueError):
                    CHECK.rewrite(self.source, self.native, self.output)
                self.assertFalse(self.output.exists())

    def test_missing_native_never_creates_output(self) -> None:
        empty = self.root / "missing-native"
        empty.mkdir()
        with self.assertRaises(FileNotFoundError):
            CHECK.rewrite(self.source, empty, self.output)
        self.assertFalse(self.output.exists())

    def test_missing_empty_duplicate_or_unsafe_aar_fails_before_output(
        self,
    ) -> None:
        for name in (
            "classes.jar",
            "AndroidManifest.xml",
            *[f"jni/{abi}/{CHECK.LIBRARY}" for abi in CHECK.ABIS],
        ):
            for missing in (True, False):
                with self.subTest(name=name, missing=missing):
                    entries = dict(self.entries)
                    if missing:
                        del entries[name]
                    else:
                        entries[name] = b""
                    self.fixture(entries)
                    with self.assertRaises(ValueError):
                        CHECK.rewrite(self.source, self.native, self.output)
                    self.assertFalse(self.output.exists())
        self.fixture(self.entries)
        with zipfile.ZipFile(self.source, "a") as archive:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                archive.writestr("classes.jar", b"duplicate")
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            CHECK.rewrite(self.source, self.native, self.output)
        self.fixture({**self.entries, "../escape": b"unsafe"})
        with self.assertRaisesRegex(ValueError, "Unsafe"):
            CHECK.rewrite(self.source, self.native, self.output)
        self.assertFalse(self.output.exists())

    def test_existing_output_is_never_overwritten(self) -> None:
        self.output.write_bytes(b"keep")
        with self.assertRaises(FileExistsError):
            CHECK.rewrite(self.source, self.native, self.output)
        self.assertEqual(self.output.read_bytes(), b"keep")

    def test_cli_emits_matching_provenance_hashes(self) -> None:
        report = self.root / "report.json"
        result = subprocess.run(
            [
                sys.executable,
                "-B",
                str(SCRIPT),
                "--source",
                str(self.source),
                "--native-dir",
                str(self.native),
                "--output",
                str(self.output),
                "--report",
                str(report),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        data = json.loads(report.read_text())
        self.assertEqual(
            data["upstream_sha256"],
            hashlib.sha256(self.source.read_bytes()).hexdigest(),
        )
        self.assertEqual(set(data["native_sha256"]), set(CHECK.ABIS))


if __name__ == "__main__":
    unittest.main()
