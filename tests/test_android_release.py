"""Android package checks reject missing or mismatched native payloads."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import warnings
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "android_release", ROOT / "tools/scripts/check-android-release.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def fixture(path, kind, missing=None, wrong_machine=False, duplicate=False):
    prefix = "" if kind == "apk" else "base/"
    names = {
        "AndroidManifest.xml" if kind == "apk"
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
    def test_both_formats_require_both_correct_native_architectures(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "package.zip"
            for kind in ("apk", "aab"):
                with self.subTest(kind=kind):
                    fixture(path, kind)
                    report = CHECK.verify(path, kind)
                    self.assertEqual(report["abis"], ["arm64-v8a", "x86_64"])
                    self.assertGreater(report["bytes"], 0)
                    prefix = "" if kind == "apk" else "base/"
                    fixture(path, kind, missing=(
                        f"{prefix}lib/x86_64/libdyapp_ffi.so"))
                    with self.assertRaisesRegex(ValueError, "missing"):
                        CHECK.verify(path, kind)
                    fixture(path, kind, wrong_machine=True)
                    with self.assertRaisesRegex(ValueError, "architecture"):
                        CHECK.verify(path, kind)
                    fixture(path, kind, duplicate=True)
                    with self.assertRaisesRegex(ValueError, "duplicate"):
                        CHECK.verify(path, kind)

    def test_empty_or_absent_payload_and_invalid_zip_fail(self):
        with tempfile.TemporaryDirectory() as directory:
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
