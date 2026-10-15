"""Practice A2: a commit that changes an interface also changes docs/.

Usage: docs-sync.py
Checks every commit after BASELINE. A commit fails when it changes a public
interface line (RULES) but no file under docs/, unless its message has a
`Docs-Unchanged: <reason>` trailer for a heuristic false positive.
"""
import importlib.util
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "triggers", ROOT / "scripts/triggers.py")
TRIGGERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRIGGERS)
# Commits after this one follow the rule.
BASELINE = "61f135ef3bba3ecef10f1fe08f83d3fab1eb9274"
# (interface, globs, changed-line regex), the shape of triggers.load().
RULES = [
    # Config keys; dyappd derives its --section.key flags from them.
    ("config key or CLI flag", ["rust/bootstrap/src/config.rs"],
     re.compile(r"^\s*pub \w+:")),
    ("CLI flag", ["rust/bootstrap/src/bin/*.rs"], re.compile(r'"--?\w')),
    ("public API", ["rust/ffi/src/*"],
     re.compile(r"^\s*pub (fn|struct|enum)\b|uniffi::")),
    ("public API", ["proto/*.proto"],
     re.compile(r"^\s*(message|enum|service|rpc)\b|=\s*\d+\s*;")),
    ("Make target", ["Makefile"], re.compile(r"^[\w.-]+:")),
]
TRAILER = re.compile(r"^Docs-Unchanged: \S", re.M)


def errors(files, message):
    """Interfaces a commit changes without docs/, or [] when it is fine."""
    if TRAILER.search(message) or any(p.startswith("docs/") for p in files):
        return []
    return TRIGGERS.fired(RULES, files)


def main():
    git = ["git", "-C", str(ROOT), "-c", f"safe.directory={ROOT}",
           "-c", "core.quotePath=false"]
    # A shallow clone without the baseline cannot run the check: fail.
    subprocess.check_call(git + ["cat-file", "-e", BASELINE + "^{commit}"])
    shas = subprocess.check_output(
        git + ["rev-list", "--no-merges", BASELINE + "..HEAD"],
        text=True).split()
    failed = []
    for sha in shas:
        message = subprocess.check_output(
            git + ["log", "-1", "--format=%B", sha], text=True)
        diff = subprocess.check_output(
            git + ["show", "--format=", "--unified=0", "--no-renames",
                   "--no-color", "--no-ext-diff", sha], text=True)
        failed += [f"{sha[:12]}: {line}"
                   for line in errors(TRIGGERS.changes(diff), message)]
    if failed:
        sys.exit("docs-sync failed: interface changed without docs/ "
                 "(docs/development/code-quality.md#docs-change-with-the-"
                 "code):\n" + "\n".join(failed))
    print("OK")


if __name__ == "__main__":
    from check_dispatch import wrap

    wrap("docs-sync")
    main()
