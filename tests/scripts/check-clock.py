"""Ratchet direct clock reads in the bootstrap node: production code reads
time through `clock.rs` so tests can drive it. Files not yet migrated keep a
budget that may only go down."""

from collections.abc import Mapping
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = Path("rust/bootstrap/src")
SEAM = SRC / "clock.rs"
READ = re.compile(r"\b(?:Instant|SystemTime|Utc)::now\b")
DECLARED = re.compile(
    r"((?:#\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;"
)
PATH = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')
INLINE = re.compile(r"^#\[cfg\(test\)\]\s*\nmod (\w+) \{", re.M)
COMMENT = re.compile(r"//.*")

# Direct reads left in files not yet moved onto the clock.
BUDGET = {
    "bin/dyappd/client.rs": 3,
    "bin/dyappd/main.rs": 1,
    "deny.rs": 3,
    "maintenance.rs": 2,
    "media.rs": 1,
    "media_repair.rs": 1,
    "metrics.rs": 1,
    "node.rs": 2,
    "node/delivery.rs": 1,
    "node/identified.rs": 2,
    "node/replicas.rs": 5,
    "node/replies.rs": 10,
    "node/run.rs": 12,
    "node/run/stored.rs": 3,
    "rate_limit.rs": 1,
    "search.rs": 1,
    "search/generation.rs": 1,
    "service/keypackages.rs": 2,
    "service/mailbox.rs": 3,
    "service/media.rs": 1,
    "service/media/catchup.rs": 1,
    "service/media/replicas.rs": 2,
    "service/media/requests.rs": 2,
    "service/revoke.rs": 3,
    "storage.rs": 2,
    "storage/keypackages.rs": 1,
}


def test_modules(sources: Mapping[Path, str]) -> set[Path]:
    """Files and module directories declared under `#[cfg(test)]`."""
    result = set()
    for path, text in sources.items():
        base = path.parent
        if path.name not in ("lib.rs", "main.rs", "mod.rs"):
            base /= path.stem
        if inline := INLINE.search(text):
            # Files an inline test module declares live under its directory.
            result.add(base / inline.group(1))
        for attrs, name in DECLARED.findall(text):
            if "#[cfg(test)]" not in attrs:
                continue
            if found := PATH.search(attrs):
                result.add(path.parent / found.group(1))
                continue
            result |= {base / f"{name}.rs", base / name / "mod.rs"}
    return result | {p.with_suffix("") for p in result}


def counts(sources: Mapping[Path, str]) -> dict[str, int]:
    """Direct reads per production file, relative to the crate source."""
    tests = test_modules(sources)
    result = {}
    for path, text in sources.items():
        if path == SEAM or any(
            p in tests for p in (path, *path.parents, path.with_suffix(""))
        ):
            continue
        if found := INLINE.search(text):
            text = text[: found.start()]
        if n := len(READ.findall(COMMENT.sub("", text))):
            result[str(path.relative_to(SRC))] = n
    return result


def errors(found: Mapping[str, int], budget: Mapping[str, int]) -> list[str]:
    result = []
    for name in sorted(found.keys() | budget.keys()):
        have, allowed = found.get(name, 0), budget.get(name, 0)
        if have > allowed:
            result.append(
                f"{SRC / name}: {have} direct clock reads, budget {allowed};"
                " read time through crate::clock::Clock"
            )
        elif have < allowed:
            result.append(
                f"{SRC / name}: {have} direct clock reads, budget {allowed};"
                " lower the budget in tests/scripts/check-clock.py"
            )
    return result


def main() -> int:
    sources = {
        path.relative_to(ROOT): path.read_text(encoding="utf-8")
        for path in sorted((ROOT / SRC).rglob("*.rs"))
    }
    found = counts(sources)
    result = errors(found, BUDGET)
    print("\n".join(result) or f"{sum(found.values())} direct reads in budget")
    return 1 if result else 0


if __name__ == "__main__":
    sys.exit(main())
