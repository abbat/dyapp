"""Every security advisory ignore carries a review date.

deny.toml reasons end in `review by YYYY-MM-DD (issue #N)`, each
osv-scanner.toml entry has `ignoreUntil`, and .cargo/audit.toml, which has no
reasons, ignores exactly the deny.toml ids. A date may be at most 90 days
ahead. make quality checks the format only, so a clean commit never turns red
as time passes; `--expiry`, run by the scheduled security workflow, also
fails on a date that has passed or is less than 14 days away.
"""
from datetime import date, timedelta
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MAX_AHEAD, WARN = timedelta(days=90), timedelta(days=14)
MARKER = re.compile(r"review by (\d{4}-\d{2}-\d{2}) \(issue #\d+\)$")


def dated(path, entries, today, expiry):
    """Errors for (id, date or None) pairs read from one file."""
    errors = []
    for vuln, day in entries:
        where = f"{path}: {vuln}"
        if day is None:
            errors.append(f"{where}: no review date")
        elif day > today + MAX_AHEAD:
            errors.append(f"{where}: review date {day} is more than 90 days "
                          "ahead")
        elif expiry and day < today + WARN:
            errors.append(f"{where}: review date {day} is due; review the "
                          "ignore, then move the date or drop it")
    return errors


def deny_dates(text):
    entries = []
    for row in tomllib.loads(text)["advisories"].get("ignore", []):
        found = MARKER.search(row.get("reason", ""))
        try:
            day = found and date.fromisoformat(found[1])
        except ValueError:
            day = None
        entries.append((row["id"], day or None))
    return entries


def osv_dates(text):
    return [(row["id"], row.get("ignoreUntil"))
            for row in tomllib.loads(text).get("IgnoredVulns", [])]


def errors(deny, audit, osvs, today, expiry=False):
    """deny and audit are file texts; osvs maps a path to its text."""
    entries = deny_dates(deny)
    found = dated("deny.toml", entries, today, expiry)
    for path, text in sorted(osvs.items()):
        found += dated(path, osv_dates(text), today, expiry)
    ignored = set(tomllib.loads(audit)["advisories"].get("ignore", []))
    if ignored != {vuln for vuln, _ in entries}:
        found.append(".cargo/audit.toml: ignore differs from deny.toml: "
                     f"{sorted(ignored ^ {vuln for vuln, _ in entries})}")
    return found


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("advisory-ignores")
    names = subprocess.check_output(
        ["git", "-C", str(ROOT), "ls-files", "*osv-scanner.toml"],
        text=True).split()
    found = errors((ROOT / "deny.toml").read_text(),
                   (ROOT / ".cargo/audit.toml").read_text(),
                   {name: (ROOT / name).read_text() for name in names},
                   date.today(), "--expiry" in sys.argv[1:])
    if found:
        found.insert(0, "Advisory ignores (docs/development/code-quality.md"
                        "#dependency-checks-cargo-deny):")
        raise ValueError("\n".join(found))
    print(f"{2 + len(names)} files: every advisory ignore has a review date")
