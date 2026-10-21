"""Apple package structure is checked without native SDKs in Docker."""
import importlib.util
from pathlib import Path
import plistlib
import stat
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "apple_package", ROOT / "tools/scripts/package-apple-release.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def app_fixture(root, target):
    app = root / "DYApp.app"
    content = app if target == "ios" else app / "Contents"
    binary = content / "DYApp" if target == "ios" else (
        content / "MacOS" / "DYApp")
    binary.parent.mkdir(parents=True)
    binary.write_bytes(b"executable fixture")
    binary.chmod(0o755)
    (content / "Info.plist").write_bytes(plistlib.dumps({
        "CFBundleExecutable": "DYApp",
        "DTPlatformName": CHECK.PLATFORMS[target][0]}))
    return app, binary


class ApplePackages(unittest.TestCase):
    def test_layout_modes_and_internal_links(self):
        for target in CHECK.PLATFORMS:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                app, binary = app_fixture(root, target)
                (binary.parent / "alias").symlink_to(binary.name)
                output = root / "app.zip"
                CHECK.package(app, target, output, CHECK.PLATFORMS[target][1])
                prefix = "Payload/" if target == "ios" else ""
                relative = binary.relative_to(app).as_posix()
                name = prefix + "DYApp.app/" + relative
                with zipfile.ZipFile(output) as archive:
                    item = archive.getinfo(name)
                    self.assertEqual(item.date_time, (1980, 1, 1, 0, 0, 0))
                    self.assertTrue(item.external_attr >> 16 & stat.S_IXUSR)
                    link_name = name.rsplit("/", 1)[0] + "/alias"
                    link = archive.getinfo(link_name)
                    self.assertTrue(stat.S_ISLNK(link.external_attr >> 16))
                    self.assertEqual(archive.read(link), b"DYApp")

    def test_rejects_simulator_and_executable_path_traversal(self):
        with tempfile.TemporaryDirectory() as directory:
            app, _ = app_fixture(Path(directory), "ios")
            for platform, executable, cause in (
                    ("iphonesimulator", "DYApp", "platform"),
                    ("iphoneos", "../secret", "basename")):
                (app / "Info.plist").write_bytes(plistlib.dumps({
                    "CFBundleExecutable": executable,
                    "DTPlatformName": platform}))
                with self.assertRaisesRegex(ValueError, cause):
                    CHECK.executable(app, "ios")

    def test_wrong_architecture_missing_binary_and_symlink_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app, binary = app_fixture(root, "ios")
            output = root / "app.ipa"
            with self.assertRaises(ValueError):
                CHECK.package(app, "ios", output, {"x86_64"})
            with self.assertRaises((ValueError, FileNotFoundError)):
                CHECK.executable(app, "macos")
            (app / "outside").symlink_to(root / "secret")
            with self.assertRaises(ValueError):
                CHECK.package(app, "ios", output, {"arm64"})
            binary.write_bytes(b"")
            with self.assertRaisesRegex(ValueError, "empty"):
                CHECK.executable(app, "ios")
            binary.write_bytes(b"fixture")
            binary.chmod(0o644)
            with self.assertRaisesRegex(ValueError, "not executable"):
                CHECK.executable(app, "ios")


if __name__ == "__main__":
    unittest.main()
