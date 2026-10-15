"""Verify Apple bundle structure and archive unsigned CI Release packages."""
import argparse
import os
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import zipfile

PLATFORMS = {
    "ios": ("iphoneos", {"arm64"}),
    "macos": ("macosx", {"arm64", "x86_64"}),
}


def executable(app, target):
    if app.is_symlink() or not app.is_dir() or app.name != "DYApp.app":
        raise ValueError("Expected a real DYApp.app directory")
    content = app if target == "ios" else app / "Contents"
    metadata = plistlib.loads((content / "Info.plist").read_bytes())
    if metadata.get("DTPlatformName") != PLATFORMS[target][0]:
        raise ValueError(f"{target}: wrong or missing build platform")
    name = metadata.get("CFBundleExecutable")
    if not isinstance(name, str) or not name or name in {".", ".."}:
        raise ValueError("Missing executable basename")
    if Path(name).name != name:
        raise ValueError("Executable must be a basename")
    binary = content / name if target == "ios" else content / "MacOS" / name
    binary.resolve().relative_to(app.resolve())
    if not binary.is_file() or binary.stat().st_size == 0:
        raise ValueError("Missing or empty application executable")
    if not binary.stat().st_mode & stat.S_IXUSR:
        raise ValueError("Application binary is not executable")
    return binary


def package(app, target, destination, architectures):
    executable(app, target)
    if set(architectures) != PLATFORMS[target][1]:
        raise ValueError(f"{target}: incorrect executable architectures")
    root = app.resolve()
    if destination.resolve().is_relative_to(root):
        raise ValueError("Archive output must be outside the app")
    members = sorted(app.rglob("*"))
    for member in members:
        if member.is_symlink():
            member.resolve().relative_to(root)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, "x") as archive:
        for member in members:
            if member.is_dir() and not member.is_symlink():
                continue
            relative = member.relative_to(app).as_posix()
            name = f"{app.name}/{relative}"
            if target == "ios":
                name = f"Payload/{name}"
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = member.lstat().st_mode << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            if member.is_symlink():
                archive.writestr(info, os.fsencode(os.readlink(member)))
            else:
                info.file_size = member.stat().st_size
                with member.open("rb") as source:
                    with archive.open(info, "w") as out:
                        shutil.copyfileobj(source, out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=PLATFORMS)
    parser.add_argument("app", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    try:
        if os.environ.get("GITHUB_ACTIONS") != "true":
            raise ValueError("Native Apple packaging requires approved CI")
        binary = executable(args.app, args.target)
        architectures = subprocess.check_output(
            ["xcrun", "lipo", "-archs", str(binary)], text=True,
            stderr=subprocess.PIPE).split()
        package(args.app, args.target, args.destination, architectures)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print("FAIL apple-package: bundle verification or archiving failed")
        print("FIX: docs/development/build.md#unsigned-apple-release-packages")
        print("DETAILS:")
        print(str(error))
        return 1
    print(f"OK apple-package {args.target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
