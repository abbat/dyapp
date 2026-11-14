"""Reject reused Android outputs and raw package differences."""

from __future__ import annotations

import importlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests/scripts"))
sys.path.insert(0, str(ROOT / "tools/scripts"))
ARTIFACTS = importlib.import_module("android_reproducible_artifacts")
CHECK = importlib.import_module("android_reproducible")
CONTAINER = importlib.import_module("android_reproducible_container")
ENVIRONMENT = importlib.import_module("android_release_environment")
STAMP = (2020, 1, 2, 3, 4, 6)


def packages(
    directory: Path, payload: bytes = b"", comment: bytes = b""
) -> None:
    for kind, name in ARTIFACTS.PACKAGES:
        prefix = "" if kind == "apk" else "base/"
        entries = {
            (
                "AndroidManifest.xml"
                if kind == "apk"
                else "base/manifest/AndroidManifest.xml"
            ): b"manifest",
            (
                "classes.dex" if kind == "apk" else "base/dex/classes.dex"
            ): b"dex",
        }
        for abi, machine in (("arm64-v8a", 183), ("x86_64", 62)):
            header = bytearray(64)
            header[:6] = b"\x7fELF\x02\x01"
            header[18:20] = machine.to_bytes(2, "little")
            entries[f"{prefix}lib/{abi}/libdyapp_ffi.so"] = (
                bytes(header) + payload
            )
        with zipfile.ZipFile(directory / name, "w") as archive:
            archive.comment = comment
            for entry, data in entries.items():
                archive.writestr(zipfile.ZipInfo(entry, STAMP), data)


class AndroidReproducibility(unittest.TestCase):
    def test_exact_packages_native_bytes_and_metadata(self) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            first, second = (
                Path(temporary) / name for name in ("first", "second")
            )
            first.mkdir()
            second.mkdir()
            packages(first)
            packages(second)
            original = ARTIFACTS.inventory(first)
            self.assertEqual(
                ARTIFACTS.changed(original, ARTIFACTS.inventory(second)), []
            )
            packages(second, comment=b"metadata only")
            changes = ARTIFACTS.changed(original, ARTIFACTS.inventory(second))
            self.assertEqual(
                changes, sorted(name for _, name in ARTIFACTS.PACKAGES)
            )
            difference = ARTIFACTS.archive_difference(
                first / ARTIFACTS.PACKAGES[0][1],
                second / ARTIFACTS.PACKAGES[0][1],
            )
            self.assertEqual(difference["entries"], [])
            self.assertNotEqual(*difference["comments"])
            packages(second, payload=b"native change")
            changes = ARTIFACTS.changed(original, ARTIFACTS.inventory(second))
            self.assertEqual(sum("!/" in name for name in changes), 4)

    def test_invalid_or_missing_packages_fail(self) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            root = Path(temporary)
            with self.assertRaises(FileNotFoundError):
                ARTIFACTS.inventory(root)
            packages(root)
            (root / ARTIFACTS.PACKAGES[0][1]).write_bytes(b"")
            with self.assertRaises(zipfile.BadZipFile):
                ARTIFACTS.inventory(root)

    def test_two_independent_outputs_and_second_output_required(self) -> None:
        for produce_second in (False, True):
            with self.subTest(produce_second=produce_second):
                self.check_pair(produce_second)

    def check_pair(self, produce_second: bool) -> None:
        seen: list[Path] = []
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            root = Path(temporary)

            def run_one(
                image: str,
                source: Path,
                destination: Path,
                log: Path,
                epoch: str,
            ) -> int:
                self.assertEqual(
                    (image, source, epoch),
                    ("immutable", root / "source", "123"),
                )
                self.assertEqual(list(destination.iterdir()), [])
                seen.append(destination)
                log.write_text("captured build")
                if len(seen) == 1 or produce_second:
                    packages(destination)
                return 0

            report: dict[str, list[dict[str, object]]] = {"builds": []}
            with patch.object(CHECK, "run_one", run_one):
                if produce_second:
                    CHECK.build_outputs(
                        root, root / "source", "immutable", "123", report
                    )
                else:
                    with self.assertRaises(FileNotFoundError):
                        CHECK.build_outputs(
                            root, root / "source", "immutable", "123", report
                        )
            self.assertEqual(len(set(seen)), 2)
            self.assertEqual(len(report["builds"]), 2)

    def test_image_validation_failure_is_reported_without_building(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            with (
                patch.object(CHECK, "output", return_value="mutable-tag"),
                patch.object(CHECK, "run_one") as run,
            ):
                report = CHECK.build_pair(Path(temporary), "a" * 40, "123")
            run.assert_not_called()
            self.assertEqual(report["builds"], [])
            self.assertIn("immutable image ID", report["errors"][0])

    def test_failed_build_retains_exit_status(self) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            report: dict[str, list[dict[str, object]]] = {"builds": []}
            with patch.object(CHECK, "run_one", return_value=7) as run:
                with self.assertRaisesRegex(ValueError, "build 1 failed"):
                    CHECK.build_outputs(
                        Path(temporary),
                        Path("source"),
                        "immutable",
                        "123",
                        report,
                    )
            self.assertEqual(run.call_count, 1)
            self.assertEqual(report["builds"][0]["exit_code"], 7)

    def test_difference_report_survives_failure(self) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            root = Path(temporary)
            report: dict[str, object] = {"builds": []}

            def run_one(
                image: str,
                source: Path,
                destination: Path,
                log: Path,
                epoch: str,
            ) -> int:
                packages(destination, comment=destination.name.encode())
                return 0

            with (
                patch.object(CHECK, "run_one", run_one),
                patch.object(CHECK, "optional_diffoscope", return_value=None),
            ):
                with self.assertRaisesRegex(ValueError, "bytes differ"):
                    CHECK.build_outputs(root, root, "immutable", "123", report)
            self.assertTrue((root / "difference.json").is_file())
            self.assertEqual(
                report["difference"], str(root / "difference.json")
            )

    def test_container_command_pins_inputs_and_isolates_workspaces(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            root = Path(temporary)
            with patch.object(CHECK.subprocess, "run") as run:
                run.return_value.returncode = 0
                for index in ("1", "2"):
                    CHECK.run_one(
                        "sha256:pinned",
                        root,
                        root / index,
                        root / f"{index}.log",
                        "123",
                    )
            commands = [call.args[0] for call in run.call_args_list]
            for command in commands:
                self.assertIn("sha256:pinned", command)
                self.assertIn(f"{root}:/source:ro", command)
                self.assertEqual(
                    command[command.index("--network") + 1], "none"
                )
                self.assertEqual(command[command.index("--epoch") + 1], "123")
            workspaces = [
                command[command.index("--workspace") + 1]
                for command in commands
            ]
            self.assertEqual(len(set(workspaces)), 2)

    def test_fresh_workspace_only_reuses_downloaded_dependencies(self) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as temporary:
            root = Path(temporary)
            source, workspace, cache = (
                root / name for name in ("source", "workspace", "cache")
            )
            source.mkdir()
            (source / "Cargo.toml").write_text("[workspace]")
            for name in ("wrapper", "caches/modules-2", "caches/transforms"):
                (cache / name).mkdir(parents=True)
            prepared = CONTAINER.prepare(source, workspace, cache, "123")
            home = Path(prepared["GRADLE_USER_HOME"])
            self.assertEqual(
                sorted(path.name for path in (home / "caches").iterdir()),
                ["modules-2"],
            )
            self.assertTrue((home / "caches/modules-2").is_symlink())
            self.assertTrue((home / "wrapper").is_symlink())
            self.assertEqual(prepared["DYAPP_ANDROID_FRESH"], "1")
            with self.assertRaisesRegex(ValueError, "start absent"):
                CONTAINER.prepare(source, workspace, cache, "123")

    def test_release_compiler_environment_is_shared_and_validates_epoch(
        self,
    ) -> None:
        original = {
            "SOURCE_DATE_EPOCH": "123",
            "RUSTFLAGS": "ambient",
            "RUSTC_WRAPPER": "sccache",
            "CFLAGS": "-g0",
        }
        prepared = ENVIRONMENT.environment(Path("/source one"), original)
        self.assertNotIn("RUSTFLAGS", prepared)
        self.assertEqual(prepared["RUSTC_WRAPPER"], "")
        self.assertEqual(prepared["CARGO_INCREMENTAL"], "0")
        self.assertIn(
            "--remap-path-prefix=/source one=/build/source",
            prepared["CARGO_ENCODED_RUSTFLAGS"],
        )
        self.assertIn("-g0", prepared["CFLAGS"])
        self.assertEqual(original["RUSTFLAGS"], "ambient")
        for epoch in ("", "-1", "1.5", "١٢٣"):
            with self.subTest(epoch=epoch):
                with self.assertRaises(ValueError):
                    ENVIRONMENT.environment(
                        Path("/source"), {"SOURCE_DATE_EPOCH": epoch}
                    )


if __name__ == "__main__":
    unittest.main()
