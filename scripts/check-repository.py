"""Nonmutating equivalents of basic repository hooks, run inside Docker."""
import json
from pathlib import Path
import subprocess

import yaml


def main():
    output = subprocess.check_output(
        ["git", "-c", "safe.directory=/workspace", "ls-files", "--cached",
         "--others", "--exclude-standard", "-z", "--", ".",
         ":(exclude).beads/**", ":(exclude).beads.gate.lock"])
    names = sorted(set(output.decode().strip("\0").split("\0")))
    folded = {}
    checked = 0
    for name in names:
        path = Path(name)
        if not path.is_file():
            continue
        key = name.casefold()
        if key in folded and folded[key] != name:
            raise ValueError(f"Case conflict: {name} / {folded[key]}")
        folded[key] = name
        data = path.read_bytes()
        if len(data) > 500 * 1024:
            raise ValueError(f"Repository file exceeds 500KiB: {name}")
        if b"\0" in data:
            continue
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            continue
        if text and not text.endswith("\n"):
            raise ValueError(f"Missing final newline: {name}")
        for number, line in enumerate(text.splitlines(), 1):
            if line.startswith(("<<<<<<< ", ">>>>>>> ")):
                raise ValueError(f"Merge conflict: {name}:{number}")
            trailing = line[len(line.rstrip(" \t")):]
            if trailing and not (path.suffix == ".md" and trailing == "  "):
                raise ValueError(f"Trailing whitespace: {name}:{number}")
        if path.suffix == ".json":
            json.loads(text)
        elif path.suffix in (".yaml", ".yml"):
            list(yaml.safe_load_all(text))
        checked += 1
    if not checked:
        raise ValueError("Empty repository validation inventory")
    print(f"{checked} text files: whitespace, conflicts, JSON/YAML valid")


if __name__ == "__main__":
    main()
