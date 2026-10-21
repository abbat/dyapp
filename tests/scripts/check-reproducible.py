"""Compare two fresh release builds in the same toolchain and environment."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    "dyappd": ("Cargo.toml", "dyappd", "Cargo.lock"),
    "linux": ("clients/linux/Cargo.toml", "dyapp-linux",
              "clients/linux/Cargo.lock"),
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


def build_pair(target, directory, revision=None, epoch=None):
    manifest, binary, lock = TARGETS[target]
    directory.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix=f"{target}-", dir=directory))
    if revision is None:
        revision = output("git", "-c", f"safe.directory={ROOT}",
                          "rev-parse", "HEAD")
        epoch = output("git", "-c", f"safe.directory={ROOT}", "show",
                       "-s", "--format=%ct", "HEAD")
    report = {
        "target": target, "revision": revision,
        "rustc": output("rustc", "-Vv"), "lock_sha256": digest(ROOT / lock),
        "source_date_epoch": epoch, "builds": [], "errors": [],
    }
    try:
        paths = []
        for index in (1, 2):
            destination = run / str(index)
            environment = os.environ.copy()
            environment.pop("RUSTFLAGS", None)
            environment.update({
                "RUSTC_WRAPPER": "",
                "RUSTC_WORKSPACE_WRAPPER": "",
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
            entry["bytes"] = executable.stat().st_size
            paths.append(executable)
        compare(paths)
    except (OSError, ValueError) as error:
        report["errors"].append(str(error))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("--target-root", type=Path,
                        default=ROOT / "build" / "reproducible")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--revision", help="Git SHA supplied by the host")
    parser.add_argument("--source-date-epoch", help="Host commit time")
    args = parser.parse_args()
    if bool(args.revision) != bool(args.source_date_epoch):
        parser.error("Use --revision and --source-date-epoch together")
    if args.revision and not all((
            re.fullmatch(r"[0-9a-f]{40}", args.revision),
            args.source_date_epoch.isdigit())):
        parser.error("Host metadata needs a full Git SHA and Unix seconds")
    report = build_pair(args.target, args.target_root.resolve(),
                        args.revision, args.source_date_epoch)
    destination = args.report or (
        ROOT / "build" / f"reproducible-{args.target}.json")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2) + "\n")
    if report["errors"]:
        print("FAIL reproducible: " + "; ".join(report["errors"]))
        print("FIX: docs/development/ci-base.md#reproducible-builds")
        print("DETAILS:")
        print("\n".join(report["errors"]))
        raise SystemExit(1)
    print(f"OK reproducible {args.target}")


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("reproducible")
    main()
