"""Check the pinned formatter and verified binary installation boundary."""
import hashlib
import importlib.util
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "install_ruff", ROOT / "tools/build-scripts/install-ruff.py")
INSTALLER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INSTALLER)


def wheel(names: tuple[str, ...]) -> bytes:
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w") as archive:
        for name in names:
            archive.writestr(name, b"fixture binary")
    return stream.getvalue()


class PythonFormatter(unittest.TestCase):
    def test_real_formatter_rejects_drift_and_accepts_canonical_input(self):
        for source, expected in (("value=1\n", 1), ("value = 1\n", 0)):
            with self.subTest(source=source):
                result = subprocess.run(
                    ["ruff", "format", "--check", "--no-cache", "--config",
                     str(ROOT / ".ruff.toml"), "--stdin-filename",
                     "fixture.py"],
                    input=source, text=True, capture_output=True, check=False)
                self.assertEqual(result.returncode, expected,
                                 result.stdout + result.stderr)

    def test_checksum_failure_preserves_existing_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "ruff"
            target.write_bytes(b"previous binary")
            with patch.object(INSTALLER, "urlopen",
                              return_value=io.BytesIO(b"wrong checksum")):
                with self.assertRaisesRegex(ValueError, "checksum"):
                    INSTALLER.install(target)
            self.assertEqual(target.read_bytes(), b"previous binary")

    def test_verified_wheel_requires_one_binary(self):
        for names in ((), ("one/ruff", "two/ruff"), ("one/ruff",)):
            with self.subTest(names=names):
                data = wheel(names)
                digest = hashlib.sha256(data).hexdigest()
                with patch.object(INSTALLER, "SHA256", digest):
                    if len(names) == 1:
                        self.assertEqual(INSTALLER.wheel_binary(data),
                                         b"fixture binary")
                    else:
                        with self.assertRaisesRegex(ValueError, "one binary"):
                            INSTALLER.wheel_binary(data)

    def test_download_size_is_bounded(self):
        with patch.object(INSTALLER, "MAX_BYTES", 4):
            with patch.object(INSTALLER, "urlopen",
                              return_value=io.BytesIO(b"12345")):
                with self.assertRaisesRegex(ValueError, "download limit"):
                    INSTALLER.install(Path("unused-destination"))


if __name__ == "__main__":
    unittest.main()
