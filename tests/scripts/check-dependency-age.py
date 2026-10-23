"""No crates.io version in a tracked Cargo.lock is younger than 14 days.

Publish times come from the sparse index cache that `cargo fetch` leaves in
CARGO_HOME, so the check runs offline in the dev container.
"""
from datetime import date, datetime, timedelta, timezone
import json
import os
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MIN_AGE = timedelta(days=14)
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
# (crate, version): (date added, reason). Remove the entry once it is 14 days
# old; an entry older than MIN_AGE is reported as stale.
LOCKED = "locked before this check existed"
EXCEPTIONS = {
    ("ml-kem", "0.3.3"): ("2026-10-23", LOCKED),
    ("proptest", "1.12.0"): ("2026-10-23", LOCKED),
    ("proptest-derive", "0.9.0"): ("2026-10-23", LOCKED),
    ("rtc", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-crypto", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-datachannel", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-dtls", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-ice", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-interceptor", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-mdns", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-media", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-rtcp", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-rtp", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-sctp", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-sdp", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-shared", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-srtp", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-stun", "0.21.1"): ("2026-10-23", LOCKED),
    ("rtc-turn", "0.21.1"): ("2026-10-23", LOCKED),
    ("rustls", "0.23.46"): ("2026-10-23", LOCKED),
    ("rustls-webpki", "0.103.16"): ("2026-10-23", LOCKED),
    ("serde_json", "1.0.154"): ("2026-10-23", LOCKED),
    ("sha2", "0.11.1"): ("2026-10-23", LOCKED),
    ("tokio", "1.53.3"): ("2026-10-23", LOCKED),
    ("toml", "1.1.8+spec-1.1.0"): ("2026-10-23", LOCKED),
    ("toml_edit", "0.25.17+spec-1.1.0"): ("2026-10-23", LOCKED),
    ("toml_parser", "1.1.5+spec-1.1.0"): ("2026-10-23", LOCKED),
    ("uuid", "1.28.0"): ("2026-10-23", LOCKED),
    ("webrtc", "0.21.1"): ("2026-10-23", LOCKED),
    ("swift-rs", "1.0.9"): ("2026-10-23", LOCKED),
    ("tauri", "2.12.3"): ("2026-10-23", LOCKED),
    ("tauri-build", "2.7.2"): ("2026-10-23", LOCKED),
    ("tauri-codegen", "2.7.2"): ("2026-10-23", LOCKED),
    ("tauri-macros", "2.7.2"): ("2026-10-23", LOCKED),
    ("tokio", "1.54.1"): ("2026-10-23", "fixes RUSTSEC-2026-0384"),
    ("ctutils", "0.4.4"): ("2026-10-23", "fixes RUSTSEC-2026-0385"),
    ("tokio-util", "0.7.20"): ("2026-10-23", LOCKED),
}


def index_path(name):
    name = name.lower()
    if len(name) <= 2:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"


def published(cache, version):
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


def packages(lockfile):
    for package in tomllib.loads(lockfile.read_text()).get("package", []):
        if package.get("source") == CRATES_IO:
            yield package["name"], package["version"]


def errors(lockfiles, indexes, now, exceptions=EXCEPTIONS):
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
            stamp = next((published(cache.read_bytes(), version)
                          for cache in caches if cache.is_file()), None)
            if stamp is None:
                found.append(f"{name} {version} ({lockfile}): not in the "
                             "index cache; rebuild the dev image")
            elif now - stamp < MIN_AGE:
                found.append(f"{name} {version} ({lockfile}): published "
                             f"{stamp:%Y-%m-%d}, younger than 14 days")
    return found


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("dependency-age")
    lockfiles = [ROOT / name for name in subprocess.check_output(
        ["git", "-C", str(ROOT), "ls-files", "*Cargo.lock"],
        text=True).split()]
    home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    indexes = sorted((home / "registry/index").glob("index.crates.io-*"))
    found = errors(lockfiles, indexes, datetime.now(timezone.utc))
    if found:
        found.insert(0, "Dependency age (docs/development/code-quality.md"
                        "#dependency-age):")
        raise ValueError("\n".join(found))
    print(f"{len(lockfiles)} lockfiles: every crates.io version is at least "
          "14 days old")
