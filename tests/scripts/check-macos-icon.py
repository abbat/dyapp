"""Checks that a macOS release ZIP carries the app icon: the bundle's
CFBundleIconFile names a file in Contents/Resources that matches the ICNS
byte for byte. Finder and the Dock take the app icon from that file.

Usage: check-macos-icon.py ZIP ICNS."""

from pathlib import Path
import plistlib
import sys
import zipfile

CONTENTS = "DYApp.app/Contents/"


def problem(archive: Path, icns: Path) -> str | None:
    with zipfile.ZipFile(archive) as bundle:
        names = set(bundle.namelist())
        if CONTENTS + "Info.plist" not in names:
            return "no DYApp.app/Contents/Info.plist"
        plist = plistlib.loads(bundle.read(CONTENTS + "Info.plist"))
        icon = plist.get("CFBundleIconFile")
        if not isinstance(icon, str) or not icon or "/" in icon:
            return "no CFBundleIconFile"
        # Info.plist may name the icon without its extension.
        member = CONTENTS + "Resources/" + icon
        if not icon.endswith(".icns"):
            member += ".icns"
        if member not in names:
            return f"{member} missing"
        if bundle.read(member) != icns.read_bytes():
            return f"{member} differs from {icns}"
    return None


def main() -> None:
    archive, icns = Path(sys.argv[1]), Path(sys.argv[2])
    found = problem(archive, icns)
    if found:
        sys.exit(f"{archive}: {found}")
    print(f"OK {archive.name} icon")


if __name__ == "__main__":
    main()
