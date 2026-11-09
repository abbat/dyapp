"""Format shared desktop Rust that Cargo fmt cannot reach through include!."""

import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("check", "write"))
    args = parser.parse_args()
    sources = sorted((ROOT / "clients/desktop").rglob("*.rs"))
    if not sources:
        parser.error("No shared desktop Rust sources found")
    command = ["rustfmt", "--edition", "2021"]
    if args.mode == "check":
        command.append("--check")
    return subprocess.run(
        [*command, *map(str, sources)], cwd=ROOT, check=False
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
