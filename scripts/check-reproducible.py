"""Compare two fresh release builds in the same toolchain and environment."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "dyappd": ("Cargo.toml", "dyappd", "Cargo.lock"),
    "linux": ("linux/Cargo.toml", "dyapp-linux", "linux/Cargo.lock"),
}


def digest(path):
    if not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"Missing or empty executable: {path}")
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def compare(paths):
    hashes = [digest(path) for path in paths]
    if len(hashes) != 2 or hashes[0] != hashes[1]:
        raise ValueError(f"Release executable hashes differ: {hashes}")
    return hashes


def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def build_pair(target, directory):
    manifest, binary, lock = TARGETS[target]
    directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix=f"{target}-", dir=directory))
    epoch = output("git", "-c", f"safe.directory={ROOT}", "show",
                   "-s", "--format=%ct", "HEAD")
    report = {
        "target": target, "revision": output(
            "git", "-c", f"safe.directory={ROOT}", "rev-parse", "HEAD"),
        "rustc": output("rustc", "-Vv"), "lock_sha256": digest(ROOT / lock),
        "source_date_epoch": epoch, "builds": [], "errors": [],
    }
    try:
        paths = []
        for index in (1, 2):
            destination = run / str(index)
            environment = os.environ.copy()
            for key in ("RUSTFLAGS", "RUSTC_WRAPPER",
                        "RUSTC_WORKSPACE_WRAPPER"):
                environment.pop(key, None)
            environment.update({
                "CARGO_INCREMENTAL": "0",
                "CARGO_TARGET_DIR": str(destination),
                "SOURCE_DATE_EPOCH": epoch,
                "CARGO_ENCODED_RUSTFLAGS": "\x1f".join([
                    f"--remap-path-prefix={ROOT}=/build/source",
                    f"--remap-path-prefix={destination}=/build/target",
                ]),
            })
            log = run / f"build-{index}.log"
            with log.open("w") as stream:
                result = subprocess.run(
                    ["cargo", "build", "--release", "--locked",
                     "--manifest-path", str(ROOT / manifest),
                     "--bin", binary],
                    cwd=ROOT, env=environment, stdout=stream,
                    stderr=subprocess.STDOUT, check=False)
            entry = {"directory": str(destination), "log": str(log),
                     "exit_code": result.returncode}
            report["builds"].append(entry)
            if result.returncode:
                raise ValueError(f"Build {index} failed; see {log}")
            executable = destination / "release" / binary
            entry["sha256"] = digest(executable)
            paths.append(executable)
        compare(paths)
    except (OSError, ValueError) as error:
        report["errors"].append(str(error))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("--target-root", type=Path,
                        default=ROOT / "target" / "reproducible")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = build_pair(args.target, args.target_root.resolve())
    destination = args.report or (
        ROOT / "target" / f"reproducible-{args.target}.json")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2) + "\n")
    if report["errors"]:
        print("FAIL reproducible: " + "; ".join(report["errors"]))
        print("FIX: docs/development/ci-base.md#reproducible-builds")
        raise SystemExit(1)
    print(f"OK reproducible {args.target}")


if __name__ == "__main__":
    main()
