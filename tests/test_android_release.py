"""Android package checks reject missing or mismatched native payloads."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
import warnings
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "android_release", ROOT / "tools/scripts/check-android-release.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def fixture(
    path: Path,
    kind: str,
    missing: str | None = None,
    wrong_machine: bool = False,
    duplicate: bool = False,
) -> None:
    prefix = "" if kind == "apk" else "base/"
    names: dict[str, bytes | bytearray] = {
        "AndroidManifest.xml"
        if kind == "apk"
        else "base/manifest/AndroidManifest.xml": b"manifest",
        "classes.dex" if kind == "apk" else "base/dex/classes.dex": b"dex",
    }
    for abi, machine in CHECK.ABIS.items():
        header = bytearray(64)
        header[:6] = b"\x7fELF\x02\x01"
        header[18:20] = (3 if wrong_machine else machine).to_bytes(2, "little")
        names[f"{prefix}lib/{abi}/libdyapp_ffi.so"] = header
    with zipfile.ZipFile(path, "w") as archive:
        for name, data in names.items():
            if name != missing:
                archive.writestr(name, data)
        if duplicate:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                archive.writestr(next(iter(names)), b"duplicate")


class AndroidPackages(unittest.TestCase):
    build = ROOT / "build/android-release-tests"

    @classmethod
    def setUpClass(cls) -> None:
        cls.build.mkdir(parents=True, exist_ok=True)

    def test_both_formats_require_both_correct_native_architectures(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory(dir=self.build) as directory:
            path = Path(directory) / "package.zip"
            for kind in ("apk", "aab"):
                with self.subTest(kind=kind):
                    fixture(path, kind)
                    report = CHECK.verify(path, kind)
                    self.assertEqual(report["abis"], ["arm64-v8a", "x86_64"])
                    self.assertGreater(report["bytes"], 0)
                    prefix = "" if kind == "apk" else "base/"
                    fixture(
                        path,
                        kind,
                        missing=(f"{prefix}lib/x86_64/libdyapp_ffi.so"),
                    )
                    with self.assertRaisesRegex(ValueError, "missing"):
                        CHECK.verify(path, kind)
                    fixture(path, kind, wrong_machine=True)
                    with self.assertRaisesRegex(ValueError, "architecture"):
                        CHECK.verify(path, kind)
                    fixture(path, kind, duplicate=True)
                    with self.assertRaisesRegex(ValueError, "duplicate"):
                        CHECK.verify(path, kind)

    def test_dependencies_require_supported_abis_and_matching_elfs(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory(dir=self.build) as directory:
            path = Path(directory) / "package.zip"
            for kind in ("apk", "aab"):
                prefix = "" if kind == "apk" else "base/"
                fixture(path, kind)
                with zipfile.ZipFile(path) as archive:
                    valid = archive.read(f"{prefix}lib/x86_64/libdyapp_ffi.so")
                wrong = bytearray(valid)
                wrong[18:20] = CHECK.ABIS["arm64-v8a"].to_bytes(2, "little")
                narrow = bytearray(valid)
                narrow[4] = 1
                cases: list[tuple[str, bytes | bytearray, str | None]] = [
                    ("x86_64/libjnidispatch.so", valid, None),
                    ("armeabi-v7a/libjnidispatch.so", valid, "unsupported"),
                    ("x86/libjnidispatch.so", valid, "unsupported"),
                    ("mips64/libjnidispatch.so", valid, "unsupported"),
                    ("x86_64/nested/libjnidispatch.so", valid, "unsupported"),
                    ("x86_64/libjnidispatch.so", wrong, "architecture"),
                    ("x86_64/libjnidispatch.so", narrow, "architecture"),
                    ("x86_64/libjnidispatch.so", valid[:32], "architecture"),
                    ("x86_64/libjnidispatch.so", b"", "empty"),
                ]
                for native, data, error in cases:
                    with self.subTest(kind=kind, native=native, error=error):
                        fixture(path, kind)
                        with zipfile.ZipFile(path, "a") as archive:
                            archive.writestr(f"{prefix}lib/{native}", data)
                        if error is None:
                            CHECK.verify(path, kind)
                        else:
                            with self.assertRaisesRegex(ValueError, error):
                                CHECK.verify(path, kind)

    def test_empty_or_absent_payload_and_invalid_zip_fail(self) -> None:
        with tempfile.TemporaryDirectory(dir=self.build) as directory:
            path = Path(directory) / "app.apk"
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr("AndroidManifest.xml", b"")
            with self.assertRaisesRegex(ValueError, "empty"):
                CHECK.verify(path, "apk")
            path.write_bytes(b"invalid archive")
            with self.assertRaises(zipfile.BadZipFile):
                CHECK.verify(path, "apk")
            with self.assertRaises(FileNotFoundError):
                CHECK.verify(Path(directory) / "missing.apk", "apk")


if __name__ == "__main__":
    unittest.main()
