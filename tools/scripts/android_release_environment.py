"""Compiler inputs shared by Android release and reproducibility builds."""

from __future__ import annotations

import os
from pathlib import Path
import shlex
import sys
from collections.abc import Mapping


def environment(root: Path, original: Mapping[str, str]) -> dict[str, str]:
    result = dict(original)
    epoch = result.get("SOURCE_DATE_EPOCH", "")
    if not epoch.isascii() or not epoch.isdigit():
        raise ValueError("SOURCE_DATE_EPOCH must contain Unix seconds")
    result.pop("RUSTFLAGS", None)
    result.update(
        {
            "RUSTC_WRAPPER": "",
            "RUSTC_WORKSPACE_WRAPPER": "",
            "CARGO_INCREMENTAL": "0",
            "CARGO_PROFILE_DEV_DEBUG": "0",
            "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(
                [
                    f"--remap-path-prefix={root}=/build/source",
                    f"--remap-path-prefix={root / 'build'}=/build/target",
                ]
            ),
        }
    )
    remaps = [
        f"-ffile-prefix-map={root}=/build/source",
        f"-ffile-prefix-map={root / 'build'}=/build/target",
    ]
    for variable in ("CFLAGS", "CXXFLAGS"):
        result[variable] = shlex.join(
            [
                *shlex.split(result.get(variable, "")),
                "-fstack-protector-strong",
                "-U_FORTIFY_SOURCE",
                "-D_FORTIFY_SOURCE=2",
                *remaps,
            ]
        )
    return result


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit(
            "Usage: android_release_environment.py ROOT COMMAND..."
        )
    try:
        prepared = environment(Path(sys.argv[1]).resolve(), os.environ)
    except ValueError as error:
        raise SystemExit(str(error)) from error
    os.execvpe(sys.argv[2], sys.argv[2:], prepared)


if __name__ == "__main__":
    main()
