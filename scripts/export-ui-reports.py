"""Copy Android test evidence from the retained reports volume."""
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def export(directory):
    containers = subprocess.check_output([
        "docker", "ps", "-aq",
        "--filter", "label=com.docker.compose.project=dyapp",
        "--filter", "label=com.docker.compose.service=android-unit-test",
    ], text=True).split()
    if not containers:
        raise RuntimeError("No Android test container; no reports to export")
    directory.mkdir(parents=True, exist_ok=True)
    destination = Path(tempfile.mkdtemp(prefix="android-", dir=directory))
    subprocess.run(["docker", "cp", f"{containers[0]}:/reports/.",
                    str(destination)], check=True)
    if not any(path.is_file() for path in destination.rglob("*")):
        raise RuntimeError(f"Empty Android report export: {destination}")
    print(f"Android reports: {destination}")
    return destination


if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("ui-reports")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path,
                        default=ROOT / "target/platform-reports")
    args = parser.parse_args()
    try:
        export(args.directory)
    except subprocess.CalledProcessError as error:
        if error.output:
            print(error.output, end="")
        code = error.returncode
        raise SystemExit(128 - code if code < 0 else code)
