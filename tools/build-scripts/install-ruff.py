"""Install the same checksum-verified Ruff binary in Docker and CI."""

import hashlib
import io
from pathlib import Path
import sys
from urllib.request import urlopen
import zipfile

VERSION = "0.17.2"
URL = (
    "https://files.pythonhosted.org/packages/c3/d8/"
    "59894cf630d2d86882457a2a6a0a1533410c5c805bfbac6f6eb4b5aef69d/"
    "ruff-0.17.2-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
)
SHA256 = "c60f93fec7142dba5b8f487f94bf2f6231dae24dc11e4c48648347c8b0720004"
MAX_BYTES = 30 * 1024 * 1024


def wheel_binary(data: bytes) -> bytes:
    if hashlib.sha256(data).hexdigest() != SHA256:
        raise ValueError("Ruff wheel checksum mismatch")
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        names = [name for name in archive.namelist() if name.endswith("/ruff")]
        if len(names) != 1:
            raise ValueError("Ruff wheel must contain exactly one binary")
        return archive.read(names[0])


def install(destination: Path) -> None:
    with urlopen(URL, timeout=60) as response:
        data = response.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError("Ruff wheel exceeds download limit")
    binary = wheel_binary(data)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(binary)
    destination.chmod(0o755)


if __name__ == "__main__":
    install(Path(sys.argv[1]))
