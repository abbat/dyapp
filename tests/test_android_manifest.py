"""Android manifest check: no backups, nothing exported but the launcher."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_manifest", ROOT / "tests/scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
MANIFEST = "clients/android/app/src/main/AndroidManifest.xml"


class Manifest(unittest.TestCase):
    def test_tracked_manifest_passes(self):
        text = (ROOT / MANIFEST).read_text()
        self.assertEqual(CHECK.manifest_errors(MANIFEST, text), [])

    def test_backup_and_exported_service_fail(self):
        text = (ROOT / MANIFEST).read_text().replace(
            'allowBackup="false"', 'allowBackup="true"').replace(
            "</application>",
            '<service android:name="S" android:exported="true" />'
            "</application>")
        self.assertEqual(CHECK.manifest_errors(MANIFEST, text), [
            f"{MANIFEST}: allowBackup is not false",
            f"{MANIFEST}: S is exported"])


if __name__ == "__main__":
    unittest.main()
