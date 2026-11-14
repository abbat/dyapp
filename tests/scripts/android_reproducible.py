"""Compare fresh Android releases in an immutable toolchain image."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile
from typing import TYPE_CHECKING

from android_reproducible_artifacts import (
    PACKAGES,
    archive_difference,
    changed,
    inventory,
)

if TYPE_CHECKING:
    import policy_contracts as pc

ROOT = Path(__file__).resolve().parents[2]
IMAGE = "dyapp:android-test"


def digest(path: Path) -> str:
    if not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"Missing or empty build input/output: {path}")
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def output(*command: str) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def source_snapshot(run: Path, revision: str) -> tuple[Path, str]:
    archive = run / "source.tar"
    subprocess.run(
        [
            "git",
            "-c",
            f"safe.directory={ROOT}",
            "archive",
            "--format=tar",
            f"--output={archive}",
            revision,
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    source = run / "source"
    source.mkdir()
    with tarfile.open(archive) as contents:
        contents.extractall(source, filter="data")
    return source, digest(archive)


def run_one(
    image: str, source: Path, destination: Path, log: Path, epoch: str
) -> int:
    command = [
        "docker",
        "run",
        "--rm",
        "--network",
        "none",
        "--cpus",
        "4",
        "--memory",
        "6g",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--hostname",
        "dyapp-reproducible",
        "--user",
        "1000:1000",
        "-e",
        "TZ=UTC",
        "-e",
        "LANG=C.UTF-8",
        "-e",
        "LC_ALL=C.UTF-8",
        "-v",
        f"{source}:/source:ro",
        "-v",
        f"{destination}:/reports",
        image,
        "python3",
        "/app/tests/scripts/android_reproducible_container.py",
        "--source",
        "/source",
        "--workspace",
        f"/app/build/reproducible/{destination.name}/source",
        "--reports",
        "/reports",
        "--epoch",
        epoch,
    ]
    with log.open("w") as stream:
        return subprocess.run(
            command,
            cwd=ROOT,
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=False,
        ).returncode


def retain_difference(
    run: Path, builds: list[pc.ReproducedBuild], names: list[str]
) -> Path:
    first, second = (Path(build["directory"]) for build in builds)
    report = {
        "changed": names,
        "archives": [
            archive_difference(first / name, second / name)
            for _, name in PACKAGES
            if name in names
        ],
    }
    destination = run / "difference.json"
    destination.write_text(json.dumps(report, indent=2) + "\n")
    return destination


def optional_diffoscope(
    run: Path, builds: list[pc.ReproducedBuild]
) -> Path | None:
    executable = shutil.which("diffoscope")
    if executable is None:
        return None
    first, second = (
        Path(build["directory"]) / PACKAGES[0][1] for build in builds
    )
    destination = run / "diffoscope.txt"
    with (run / "diffoscope.log").open("w") as stream:
        subprocess.run(
            [executable, "--text", str(destination), str(first), str(second)],
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=False,
        )
    return destination if destination.is_file() else None


def build_outputs(
    run: Path,
    source: Path,
    image: str,
    epoch: str,
    report: pc.ReproducibilityReport,
) -> None:
    inventories: list[dict[str, pc.ReproducedArtifact]] = []
    for index in (1, 2):
        destination = run / str(index)
        destination.mkdir()
        destination.chmod(
            0o777
        )  # Only this new report mount is writable by the image's UID 1000.
        log = run / f"build-{index}.log"
        entry: pc.ReproducedBuild = {
            "directory": str(destination),
            "log": str(log),
            "exit_code": -1,
        }
        report["builds"].append(entry)
        entry["exit_code"] = run_one(image, source, destination, log, epoch)
        if entry["exit_code"]:
            raise ValueError(f"Android build {index} failed; see {log}")
        artifacts = inventory(destination)
        inventories.append(artifacts)
        entry["artifacts"] = artifacts
        apk = artifacts[PACKAGES[0][1]]
        entry["sha256"], entry["bytes"] = apk["sha256"], apk["bytes"]
    names = changed(inventories[0], inventories[1])
    if names:
        report["difference"] = str(
            retain_difference(run, report["builds"], names)
        )
        diagnostic = optional_diffoscope(run, report["builds"])
        if diagnostic is not None:
            report["diffoscope"] = str(diagnostic)
        raise ValueError("Android release bytes differ: " + ", ".join(names))


def build_pair(
    directory: Path, revision: str | None, epoch: str | None
) -> pc.ReproducibilityReport:
    directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="android-", dir=directory))
    report: pc.ReproducibilityReport = {
        "target": "android",
        "revision": revision,
        "rustc": "",
        "lock_sha256": "",
        "source_date_epoch": epoch,
        "builds": [],
        "errors": [],
    }
    try:
        if revision is None:
            revision = output(
                "git", "-c", f"safe.directory={ROOT}", "rev-parse", "HEAD"
            )
            epoch = output(
                "git",
                "-c",
                f"safe.directory={ROOT}",
                "show",
                "-s",
                "--format=%ct",
                "HEAD",
            )
        if epoch is None:
            raise ValueError("Source epoch is required")
        report["revision"], report["source_date_epoch"] = revision, epoch
        image = output(
            "docker", "image", "inspect", "--format", "{{.Id}}", IMAGE
        )
        if re.fullmatch(r"sha256:[0-9a-f]{64}", image) is None:
            raise ValueError(
                "Prepared Android image needs an immutable image ID"
            )
        report["image_id"] = image
        report["rustc"] = output(
            "docker", "run", "--rm", "--network", "none", image, "rustc", "-Vv"
        )
        source, sha = source_snapshot(run, revision)
        report["source_archive_sha256"] = sha
        report["lock_sha256"] = digest(source / "Cargo.lock")
        build_outputs(run, source, image, epoch, report)
    except (
        OSError,
        ValueError,
        RuntimeError,
        subprocess.CalledProcessError,
        tarfile.TarError,
        zipfile.BadZipFile,
    ) as error:
        report["errors"].append(str(error))
    return report
