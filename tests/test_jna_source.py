"""Pinned JNA sources fail closed before executing changed or unsafe inputs."""

from dataclasses import asdict, replace
import hashlib
import importlib.util
from io import BytesIO
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "jna_source", ROOT / "tools/scripts/jna_source.py"
)
assert SPEC is not None and SPEC.loader is not None
SOURCE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = SOURCE
SPEC.loader.exec_module(SOURCE)


class JnaSource(unittest.TestCase):
    def setUp(self) -> None:
        build = ROOT / "build/jna-source-tests"
        build.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=build)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.original = self.root / "inputs"
        self.original.mkdir()
        self.commit = "1" * 40
        self.prefix = "jna-" + self.commit
        self.tar = self.original / "source.tar.gz"
        self.archive(
            [
                ("native/libffi/autogen.sh", b"bootstrap"),
                ("src/com/sun/jna/Native.java", b"source"),
                ("lib/unneeded.dll", b"not extracted"),
            ]
        )
        with zipfile.ZipFile(self.original / "upstream.aar", "w") as archive:
            archive.writestr("classes.jar", b"original Java")
        self.inputs = SOURCE.Inputs(
            version="5.19.1",
            native_version="7.0.4",
            commit=self.commit,
            source_sha256=SOURCE.digest(self.tar),
            aar_sha256=SOURCE.digest(self.original / "upstream.aar"),
            classes_sha256=hashlib.sha256(b"original Java").hexdigest(),
            ndk_version="29.0.14206865",
        )
        self.config = self.root / "inputs.json"
        self.config.write_text(json.dumps(asdict(self.inputs)))
        self.work = self.root / "work"

    def archive(self, entries: list[tuple[str, bytes]]) -> None:
        with tarfile.open(self.tar, "w:gz") as archive:
            for name, data in entries:
                info = tarfile.TarInfo(f"{self.prefix}/{name}")
                info.size = len(data)
                info.mode = 0o6755
                archive.addfile(info, BytesIO(data))

    def test_config_fields_and_pin_formats_are_validated(self) -> None:
        self.assertEqual(SOURCE.load(self.config), self.inputs)
        for changed in [
            {**asdict(self.inputs), "source_sha256": "invalid"},
            {**asdict(self.inputs), "commit": "../unexpected"},
            {**asdict(self.inputs), "version": 5},
            {**asdict(self.inputs), "url": "https://untrusted.invalid"},
        ]:
            self.config.write_text(json.dumps(changed))
            with self.assertRaises(ValueError):
                SOURCE.load(self.config)

    def test_extracts_only_regular_sources_without_special_modes(self) -> None:
        output = self.root / "source"
        SOURCE.extract(self.tar, output, self.commit)
        native = output / "native/libffi/autogen.sh"
        self.assertEqual(native.read_bytes(), b"bootstrap")
        self.assertEqual(native.stat().st_mode & 0o7777, 0o755)
        self.assertFalse((output / "lib").exists())

    def test_paths_links_duplicates_and_special_files_are_rejected(
        self,
    ) -> None:
        for index, kind in enumerate(
            [
                "traversal",
                "absolute",
                "symlink",
                "hardlink",
                "fifo",
                "duplicate",
            ]
        ):
            with self.subTest(kind=kind):
                with tarfile.open(self.tar, "w:gz") as archive:
                    name = f"{self.prefix}/native/file"
                    if kind == "traversal":
                        name = f"{self.prefix}/native/../../escape"
                    if kind == "absolute":
                        name = "/" + name
                    info = tarfile.TarInfo(name)
                    types = {
                        "symlink": tarfile.SYMTYPE,
                        "hardlink": tarfile.LNKTYPE,
                        "fifo": tarfile.FIFOTYPE,
                    }
                    info.type = types.get(kind, tarfile.REGTYPE)
                    info.linkname = "../../escape"
                    archive.addfile(info)
                    if kind == "duplicate":
                        archive.addfile(info)
                with self.assertRaises(ValueError):
                    SOURCE.extract(
                        self.tar, self.root / str(index), self.commit
                    )
                self.assertFalse((self.root / "escape").exists())

    def test_bad_input_hash_blocks_bootstrap_and_success_receipt(self) -> None:
        for field in ("source_sha256", "aar_sha256", "classes_sha256"):
            work = self.root / field
            with (
                self.subTest(field=field),
                patch.object(SOURCE.subprocess, "run") as run,
            ):
                with self.assertRaises(ValueError):
                    SOURCE.prepare(
                        replace(self.inputs, **{field: "0" * 64}),
                        self.original,
                        work,
                    )
                run.assert_not_called()
            self.assertFalse((work / "prepared.json").exists())

    def test_prepared_snapshot_rejects_changed_and_added_inputs(self) -> None:
        with patch.object(SOURCE.subprocess, "run"):
            SOURCE.prepare(self.inputs, self.original, self.work)
        SOURCE.validate(self.inputs, self.work)
        with self.assertRaises(ValueError):
            SOURCE.validate(
                replace(self.inputs, native_version="1.0.0"), self.work
            )
        for name in ("src/com/sun/jna/Native.java", "native/new.c"):
            target = self.work / "source" / name
            before = target.read_bytes() if target.exists() else None
            target.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                SOURCE.validate(self.inputs, self.work)
            if before is not None:
                target.write_bytes(before)

    def test_fetch_uses_fixed_origins_and_verifies_received_bytes(
        self,
    ) -> None:
        responses = [
            BytesIO(self.tar.read_bytes()),
            BytesIO((self.original / "upstream.aar").read_bytes()),
        ]
        target = self.root / "downloads"
        with patch.object(SOURCE, "urlopen", side_effect=responses) as request:
            SOURCE.fetch(self.inputs, target)
        self.assertEqual(
            SOURCE.digest(target / "source.tar.gz"), self.inputs.source_sha256
        )
        self.assertEqual(
            SOURCE.digest(target / "upstream.aar"), self.inputs.aar_sha256
        )
        self.assertEqual(request.call_count, 2)
        self.assertIn("codeload.github.com", request.call_args_list[0].args[0])
        self.assertIn(
            "repo.maven.apache.org", request.call_args_list[1].args[0]
        )

    def test_source_mirror_is_used_after_primary_failure(self) -> None:
        responses = [
            OSError("primary unavailable"),
            BytesIO(self.tar.read_bytes()),
            BytesIO((self.original / "upstream.aar").read_bytes()),
        ]
        target = self.root / "mirror-downloads"
        with patch.object(SOURCE, "urlopen", side_effect=responses) as request:
            SOURCE.fetch(self.inputs, target)
        self.assertEqual(request.call_count, 3)
        self.assertIn("codeload.github.com", request.call_args_list[0].args[0])
        self.assertIn("gh-proxy.com", request.call_args_list[1].args[0])
        self.assertEqual(
            SOURCE.digest(target / "source.tar.gz"), self.inputs.source_sha256
        )

    def test_aar_mirror_is_used_after_primary_failure(self) -> None:
        responses = [
            BytesIO(self.tar.read_bytes()),
            OSError("primary unavailable"),
            BytesIO((self.original / "upstream.aar").read_bytes()),
        ]
        target = self.root / "aar-mirror-downloads"
        with patch.object(SOURCE, "urlopen", side_effect=responses) as request:
            SOURCE.fetch(self.inputs, target)
        self.assertEqual(request.call_count, 3)
        self.assertIn("googleapis.com", request.call_args_list[2].args[0])
        self.assertEqual(
            SOURCE.digest(target / "upstream.aar"), self.inputs.aar_sha256
        )

    def test_transfer_rejects_bad_hash_budget_and_existing_destinations(
        self,
    ) -> None:
        for name, checksum, limit in [
            ("bad", "0" * 64, SOURCE.MAX_INPUT),
            ("budget", hashlib.sha256(b"ab").hexdigest(), 1),
        ]:
            with (
                self.subTest(name=name),
                patch.object(SOURCE, "MAX_INPUT", limit),
            ):
                with self.assertRaises(ValueError):
                    SOURCE.transfer(BytesIO(b"ab"), self.root / name, checksum)
        existing = self.root / "existing"
        existing.write_bytes(b"keep")
        with self.assertRaises(FileExistsError):
            SOURCE.transfer(BytesIO(b"ab"), existing, "0" * 64)
        self.assertEqual(existing.read_bytes(), b"keep")

    def test_native_cli_refuses_changed_source_before_publishing(self) -> None:
        with patch.object(SOURCE.subprocess, "run"):
            SOURCE.prepare(self.inputs, self.original, self.work)
        (self.work / "source/native/new.c").write_bytes(b"changed")
        output = self.root / "artifact"
        result = subprocess.run(
            [
                sys.executable,
                "-B",
                str(ROOT / "tools/scripts/jna_native.py"),
                "--config",
                str(self.config),
                "--work",
                str(self.work),
                "--ndk",
                str(self.root / "unused-ndk"),
                "--output",
                str(output),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Prepared JNA source changed", result.stderr)
        self.assertFalse((output / "manifest.json").exists())


if __name__ == "__main__":
    unittest.main()
