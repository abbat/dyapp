"""Pack the release Windows client into a portable ZIP.

The ZIP holds dyapp.exe and LICENSE in one folder.

Usage: package-windows-zip.py EXE OUT_DIR. Prints the ZIP path and its SHA-256.
"""
import hashlib
from pathlib import Path
import re
import sys
import zipfile


def main():
    exe, out = Path(sys.argv[1]), Path(sys.argv[2])
    root = Path(__file__).resolve().parents[2]
    manifest = (root / "clients" / "windows" / "Cargo.toml").read_text()
    version = re.search(r'^version = "(.*)"', manifest, re.M).group(1)
    name = f"dyapp-{version}-x86_64-windows"
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{name}.zip"
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        # A fixed timestamp keeps the ZIP reproducible from the same EXE.
        members = ((exe, "dyapp.exe"), (root / "LICENSE", "LICENSE"))
        for source, member in members:
            info = zipfile.ZipInfo(f"{name}/{member}", (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, source.read_bytes())
    print(path, hashlib.sha256(path.read_bytes()).hexdigest())


if __name__ == "__main__":
    main()
