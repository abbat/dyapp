"""No crates.io version in a tracked Cargo.lock is younger than 14 days.

Publish times come from the sparse index cache that `cargo fetch` leaves in
CARGO_HOME, so the check runs offline in the dev container.
"""

import typing

from datetime import date, datetime, timedelta, timezone
import json
import os
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MIN_AGE = timedelta(days=14)
ExceptionPolicy = typing.Mapping[tuple[str, str], tuple[str, str]]
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
# (crate, version): (date added, reason). Remove the entry once it is 14 days
# old; an entry older than MIN_AGE is reported as stale.
EXCEPTIONS: dict[tuple[str, str], tuple[str, str]] = {
    ("url", "2.5.9"): (
        "2026-11-22",
        "RUSTSEC-2026-0592 security fix; remove after 2026-12-04",
    ),
    ("form_urlencoded", "1.2.3"): (
        "2026-11-22",
        "required by url 2.5.9 security fix; remove after 2026-12-04",
    ),
}


def index_path(name: "str") -> "str":
    name = name.lower()
    if len(name) <= 2:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"


def published(cache: "bytes", version: "str") -> "datetime | None":
    """Publish time of version in a sparse index cache file, None if absent.

    The file is a binary header then NUL-separated version/JSON pairs."""
    for chunk in cache.split(b"\0"):
        if not chunk.startswith(b"{"):
            continue
        entry = json.loads(chunk)
        if entry["vers"] != version:
            continue
        # Versions published before crates.io recorded pubtime are years old.
        stamp = entry.get("pubtime", "1970-01-01T00:00:00Z")
        return datetime.fromisoformat(stamp.replace("Z", "+00:00"))
    return None


def packages(lockfile: "Path") -> "typing.Iterator[tuple[str, str]]":
    for package in tomllib.loads(lockfile.read_text()).get("package", []):
        if package.get("source") == CRATES_IO:
            yield package["name"], package["version"]


def errors(
    lockfiles: "list[Path]",
    indexes: "list[Path]",
    now: "datetime",
    exceptions: "ExceptionPolicy" = EXCEPTIONS,
) -> "list[str]":
    found = []
    for (name, version), (added, reason) in exceptions.items():
        if not reason:
            found.append(f"exception {name} {version} has no reason")
        if now.date() - date.fromisoformat(added) > MIN_AGE:
            found.append(f"exception {name} {version} is stale: remove it")
    seen = set()
    for lockfile in lockfiles:
        for name, version in packages(lockfile):
            if (name, version) in seen or (name, version) in exceptions:
                continue
            seen.add((name, version))
            caches = [index / ".cache" / index_path(name) for index in indexes]
            stamp = next(
                (
                    published(cache.read_bytes(), version)
                    for cache in caches
                    if cache.is_file()
                ),
                None,
            )
            if stamp is None:
                found.append(
                    f"{name} {version} ({lockfile}): not in the "
                    "index cache; rebuild the dev image"
                )
            elif now - stamp < MIN_AGE:
                found.append(
                    f"{name} {version} ({lockfile}): published "
                    f"{stamp:%Y-%m-%d}, younger than 14 days"
                )
    return found


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("dependency-age")
    lockfiles = [
        ROOT / name
        for name in subprocess.check_output(
            ["git", "-C", str(ROOT), "ls-files", "*Cargo.lock"], text=True
        ).split()
    ]
    home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    indexes = sorted((home / "registry/index").glob("index.crates.io-*"))
    found = errors(lockfiles, indexes, datetime.now(timezone.utc))
    if found:
        found.insert(
            0,
            "Dependency age (docs/development/code-quality.md"
            "#dependency-age):",
        )
        raise ValueError("\n".join(found))
    print(
        f"{len(lockfiles)} lockfiles: every crates.io version is at least "
        "14 days old"
    )
