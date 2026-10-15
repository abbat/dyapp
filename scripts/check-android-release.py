"""Check unsigned Android Release APK/AAB structure and native ABI payloads."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ABIS = {"arm64-v8a": 183, "x86_64": 62}


def verify(path, kind):
    prefix = "" if kind == "apk" else "base/"
    manifest = ("AndroidManifest.xml" if kind == "apk"
                else "base/manifest/AndroidManifest.xml")
    dex = "classes.dex" if kind == "apk" else "base/dex/classes.dex"
    libraries = {f"{prefix}lib/{abi}/libdyapp_ffi.so": machine
                 for abi, machine in ABIS.items()}
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError(f"{kind}: duplicate ZIP entries")
        for name in [manifest, dex, *libraries]:
            if name not in names or archive.getinfo(name).file_size == 0:
                raise ValueError(f"{kind}: missing or empty {name}")
        for name, machine in libraries.items():
            with archive.open(name) as stream:
                header = stream.read(64)
            valid = all((
                len(header) == 64, header[:4] == b"\x7fELF",
                header[4:6] == b"\x02\x01",
                int.from_bytes(header[18:20], "little") == machine))
            if not valid:
                raise ValueError(f"{kind}: wrong ELF architecture: {name}")
    with path.open("rb") as stream:
        sha = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "bytes": path.stat().st_size,
            "sha256": sha, "abis": sorted(ABIS)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apk", required=True, type=Path)
    parser.add_argument("--aab", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    report = {"expected_profile": "release", "signing": "not-checked",
              "artifacts": {}, "errors": []}
    for kind in ("apk", "aab"):
        try:
            report["artifacts"][kind] = verify(getattr(args, kind), kind)
        except (OSError, ValueError, RuntimeError,
                zipfile.BadZipFile) as error:
            report["errors"].append(str(error))
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    if report["errors"]:
        print("FAIL android-release: package verification failed")
        print("FIX: docs/development/build.md#unsigned-android-release")
        print("DETAILS:")
        print("\n".join(report["errors"]))
        return 1
    print("OK android-release packages")
    return 0


if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("android-release")
    raise SystemExit(main())
