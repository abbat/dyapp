"""Exercise verified installation boundaries and both real strict gates."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import stat
import tarfile
from tempfile import TemporaryDirectory
from types import ModuleType
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name: str, relative: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


INSTALL = load("install_mypy", "tools/build-scripts/install-mypy.py")
CHECK = load("check_python_types", "tools/scripts/check-python-types.py")


def wheel(files: dict[str, bytes]) -> bytes:
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w") as archive:
        for name, value in files.items():
            archive.writestr(name, value)
    return output.getvalue()


def artifact(data: bytes, kind: str = "wheel") -> dict[str, str]:
    return {
        "project": "fixture",
        "version": "2.20.0",
        "abi": "any",
        "kind": kind,
        "url": "https://files.pythonhosted.org/fixture.whl",
        "sha256": hashlib.sha256(data).hexdigest(),
    }


class PythonTypeInstallation(unittest.TestCase):
    def test_supported_abis_have_complete_distinct_locked_dependencies(
        self,
    ) -> None:
        lock = INSTALL.load_lock(INSTALL.LOCK)
        for abi in ("cp311", "cp312", "cp313"):
            with self.subTest(abi=abi):
                selected = INSTALL.select(lock, abi, "linux", "x86_64")
                names = [entry["project"] for entry in selected]
                self.assertEqual(len(names), len(set(names)))
                self.assertEqual(set(names), INSTALL.PROJECTS)
                for entry in selected:
                    self.assertRegex(entry["sha256"], r"^[0-9a-f]{64}$")
        for abi, system, machine in (
            ("cp314", "linux", "x86_64"),
            ("cp313", "darwin", "x86_64"),
            ("cp313", "linux", "aarch64"),
        ):
            with self.subTest(abi=abi, system=system, machine=machine):
                with self.assertRaisesRegex(ValueError, "Unsupported"):
                    INSTALL.select(lock, abi, system, machine)
        self.assertEqual(lock["gi_versions"]["Gtk"], "3")
        self.assertEqual(lock["gi_versions"]["Gdk"], "3")
        self.assertEqual(lock["gi_versions"]["Soup"], "3")

    def test_checksum_failure_writes_no_installation(self) -> None:
        expected = artifact(b"expected archive")
        with TemporaryDirectory() as directory:
            destination = Path(directory) / "libraries"
            with (
                patch.object(
                    INSTALL, "load_lock", return_value={"gi_versions": {}}
                ),
                patch.object(INSTALL, "select", return_value=[expected]),
                patch.object(
                    INSTALL, "urlopen", return_value=io.BytesIO(b"corrupted")
                ),
            ):
                with self.assertRaisesRegex(ValueError, "checksum"):
                    INSTALL.install(destination)
            self.assertFalse(destination.exists())

    def test_archive_traversal_and_special_files_are_rejected(self) -> None:
        for name in (
            "../escape.py",
            "/escape.py",
            "x/../escape.py",
            "x\\escape.py",
        ):
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, "archive path"):
                    list(INSTALL.wheel_files(wheel({name: b"source"})))
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w") as archive:
            link = zipfile.ZipInfo("linked.py")
            link.external_attr = (stat.S_IFLNK | 0o777) << 16
            archive.writestr(link, "../outside")
        with self.assertRaisesRegex(ValueError, "special file"):
            list(INSTALL.wheel_files(output.getvalue()))

    def test_duplicate_and_oversized_payloads_fail_before_replacing_content(
        self,
    ) -> None:
        data = wheel({"owned.py": b"new"})
        contents = {"owned.py": b"existing"}
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            INSTALL.unpack(artifact(data), data, contents)
        self.assertEqual(contents, {"owned.py": b"existing"})
        with patch.object(INSTALL, "MAX_FILE", 2):
            with self.assertRaisesRegex(ValueError, "size limit"):
                list(INSTALL.wheel_files(data))
        with patch.object(INSTALL, "MAX_DOWNLOAD", 2):
            with patch.object(
                INSTALL, "urlopen", return_value=io.BytesIO(b"123")
            ):
                with self.assertRaisesRegex(ValueError, "download limit"):
                    INSTALL.download(artifact(b"123"))

    def test_gi_source_keeps_license_and_selects_requested_api(self) -> None:
        output = io.BytesIO()
        prefix = "pygobject_stubs-2.20.0/"
        files = {
            prefix + "LICENSE": b"upstream license",
            prefix + "src/gi-stubs/repository/_Gtk3.pyi": b"class Widget: ...",
            prefix + "pep517backend/backend.py": b"must not execute",
        }
        with tarfile.open(fileobj=output, mode="w:gz") as archive:
            for name, value in files.items():
                member = tarfile.TarInfo(name)
                member.size = len(value)
                archive.addfile(member, io.BytesIO(value))
        contents = dict(INSTALL.gi_files(output.getvalue(), "2.20.0"))
        INSTALL.configure_gi(contents, {"Gtk": "3"})
        self.assertEqual(contents["gi-stubs/LICENSE"], b"upstream license")
        self.assertEqual(
            contents["gi-stubs/repository/Gtk.pyi"],
            b"from ._Gtk3 import *",
        )
        self.assertEqual(len(contents), 3)
        with self.assertRaisesRegex(ValueError, "Missing configured"):
            INSTALL.configure_gi(contents, {"Gtk": "4"})


class PythonTypeGate(unittest.TestCase):
    def fixture(self, root: Path) -> None:
        files = {
            "tools/common.py": "def value() -> int:\n    return 1\n",
            "tools/api.pyi": "def value() -> int: ...\n",
            "tests/scripts/windows-tray.py": (
                'import ctypes\nctypes.WinDLL("user32")\n'
            ),
            "tests/scripts/check-windows-icon.py": (
                "import ctypes\nctypes.WINFUNCTYPE(ctypes.c_int)\n"
            ),
        }
        for name, text in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        lock = root / "tools/python-types/requirements.json"
        lock.parent.mkdir(parents=True)
        lock.write_bytes(
            (ROOT / "tools/python-types/requirements.json").read_bytes()
        )
        (root / ".mypy.ini").write_bytes((ROOT / ".mypy.ini").read_bytes())

    def test_every_source_and_local_stub_has_exactly_one_platform(
        self,
    ) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            targets = CHECK.groups(root)
            self.assertEqual(
                targets["linux"], ["tools/api.pyi", "tools/common.py"]
            )
            self.assertEqual(len(targets["win32"]), 2)
            extra = root / "tests/new-helper.py"
            extra.write_text("answer = 42\n")
            self.assertIn("tests/new-helper.py", CHECK.groups(root)["linux"])

    def test_real_checker_rejects_linux_and_windows_regressions(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            self.assertEqual(CHECK.check(root), 0)
            common = root / "tools/common.py"
            common.write_text(
                'def value() -> int:\n    return "wrong result"\n'
            )
            self.assertEqual(CHECK.check(root), 1)
            common.write_text("def value() -> int:\n    return 1\n")
            windows = root / "tests/scripts/windows-tray.py"
            windows.write_text(
                'import ctypes\nctypes.DoesNotExist("user32")\n'
            )
            self.assertEqual(CHECK.check(root), 1)

    def test_stale_installation_cannot_run_a_different_lock(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            library = root / "libraries"
            library.mkdir()
            (library / ".dyapp-python-types.json").write_text(
                json.dumps({"abi": "cp313", "lock_sha256": "wrong"})
            )
            with self.assertRaisesRegex(ValueError, "stale"):
                CHECK.environment(library, root)


if __name__ == "__main__":
    unittest.main()
