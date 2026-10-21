"""Practice A6a: check the trigger list and report the triggers a diff fires.

Usage: triggers.py [GIT-DIFF-ARGS...]
Without arguments only checks that every glob matches a tracked file.
With arguments (e.g. `origin/main...HEAD`, `--cached`) also prints
`trigger: path` for every trigger the `git diff` fires.
"""
from fnmatch import fnmatchcase
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
LIST = ROOT / "tests/scripts/triggers.toml"


def load(text):
    """Triggers as (name, globs, compiled line regex or None)."""
    return [(t["name"], t["paths"], re.compile(t["lines"]) if "lines" in t
             else None) for t in tomllib.loads(text)["trigger"]]


def uncovered(triggers, files):
    """Globs that match no tracked file."""
    return [f"{name}: {glob}" for name, globs, _ in triggers for glob in globs
            if not any(fnmatchcase(f, glob) for f in files)]


def changes(diff):
    """{path: changed lines} from `git diff --unified=0 --no-renames`."""
    files, current, header = {}, [], True
    for line in diff.splitlines():
        if line.startswith("diff --git a/"):
            # Without renames the line is `a/PATH b/PATH`.
            pair = line[len("diff --git a/"):]
            current = files.setdefault(pair[:(len(pair) - 3) // 2], [])
            header = True
        elif line.startswith("@@"):
            header = False
        elif not header and line.startswith(("+", "-")):
            current.append(line[1:])
    return files


def fired(triggers, files):
    """Sorted `trigger: path` lines for the changed files."""
    def fires(globs, regex, path, lines):
        if not any(fnmatchcase(path, glob) for glob in globs):
            return False
        return regex is None or any(map(regex.search, lines))

    return sorted({f"{name}: {path}" for name, globs, regex in triggers
                   for path, lines in files.items()
                   if fires(globs, regex, path, lines)})


def main(args):
    triggers = load(LIST.read_text())
    git = ["git", "-C", str(ROOT), "-c", f"safe.directory={ROOT}",
           "-c", "core.quotePath=false"]
    tracked = subprocess.check_output(
        git + ["ls-files"], text=True).splitlines()
    missing = uncovered(triggers, tracked)
    if missing:
        sys.exit("trigger globs match no tracked file:\n" + "\n".join(missing))
    if args:
        diff = subprocess.check_output(
            git + ["diff", "--unified=0", "--no-renames", "--no-color",
                   "--no-ext-diff", *args], text=True)
        print("\n".join(fired(triggers, changes(diff))) or "no triggers")


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("triggers", data=bool(sys.argv[1:]))
    main(sys.argv[1:])
