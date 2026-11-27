"""Installer refusal tests; native proof validation remains separate."""

from hashlib import sha256
import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).parent / "scripts" / "android_node_fixture.py"
SPEC = importlib.util.spec_from_file_location("android_node_fixture", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class NodeFixtureTests(unittest.TestCase):
    def test_install_preserves_exact_bytes_and_refuses_existing_destination(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, destination = root / "fixture", root / "node.key"
            data = b"public fixture bytes"
            source.write_bytes(data)
            digest = sha256(data).hexdigest()
            MODULE.install(source, destination, digest)
            self.assertEqual(destination.read_bytes(), data)
            self.assertEqual(destination.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                MODULE.install(source, destination, digest)
            self.assertEqual(destination.read_bytes(), data)

    def test_invalid_inputs_never_create_destination(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, destination = root / "fixture", root / "node.key"
            for data, digest in [
                (b"changed", sha256(b"original").hexdigest()),
                (b"", sha256(b"").hexdigest()),
                (b"x" * 4097, sha256(b"x" * 4097).hexdigest()),
                (b"data", "not-a-digest"),
            ]:
                source.write_bytes(data)
                with self.assertRaises(ValueError):
                    MODULE.install(source, destination, digest)
                self.assertFalse(destination.exists())

    def test_symlinks_never_redirect_fixture_installation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, destination = root / "fixture", root / "node.key"
            original = root / "original"
            original.write_bytes(b"public fixture")
            digest = sha256(original.read_bytes()).hexdigest()
            source.symlink_to(original)
            with self.assertRaises(ValueError):
                MODULE.install(source, destination, digest)
            self.assertFalse(destination.exists())
            destination.symlink_to(original)
            with self.assertRaises(FileExistsError):
                MODULE.install(original, destination, digest)
            self.assertEqual(original.read_bytes(), b"public fixture")
            self.assertTrue(destination.is_symlink())

    def test_missing_fixture_never_creates_destination(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / "node.key"
            with self.assertRaises(ValueError):
                MODULE.install(root / "missing", destination, "0" * 64)
            self.assertFalse(destination.exists())
