"""Practice A4: count changed text lines per non-merge commit."""
import argparse
import json
from pathlib import Path, PurePosixPath
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def git(*args):
    return subprocess.check_output(
        ["git", "-c", f"safe.directory={Path.cwd()}", *args])


def excluded(name):
    path = PurePosixPath(name)
    return any((
        path.name.endswith((".lock", ".lockfile", ".snap", ".uniffi.rs")),
        path.name in {"package-lock.json", "npm-shrinkwrap.json"},
        set(path.parts) & {"generated", "snapshots", "__snapshots__"},
    ))


def changed_lines(stats):
    """Read --numstat -z without renames; binary entries have no lines."""
    total = 0
    for entry in filter(None, stats.split(b"\0")):
        added, removed, name = entry.split(b"\t", 2)
        if not excluded(name.decode("utf-8", errors="surrogateescape")):
            if added != b"-":
                total += int(added) + int(removed)
    return total


def inspect(base, head, limit):
    # Missing/divergent baselines and incomplete history must fail, not skip.
    if git("rev-parse", "--is-shallow-repository").strip() != b"false":
        raise ValueError("Small batches require a full-history checkout")
    subprocess.check_call(
        ["git", "-c", f"safe.directory={Path.cwd()}", "merge-base",
         "--is-ancestor", base, head])
    commits, errors = [], []
    for sha in git("rev-list", "--no-merges", f"{base}..{head}").split():
        sha = sha.decode("ascii")
        lines = changed_lines(git(
            "diff-tree", "--root", "--no-commit-id", "-r", "--numstat",
            "-z", "--no-renames", "--no-ext-diff", "--no-textconv", sha))
        reason = git(
            "show", "-s", "--format=%(trailers:key=Large-Change,valueonly)",
            sha).decode().strip()
        large = lines > limit
        commits.append({"commit": sha, "lines": lines, "large": large,
                        "reason": reason})
        if large and not reason:
            errors.append(f"{sha[:12]}: {lines} > {limit} changed lines; "
                          "split the commit or add Large-Change: <reason>")
    large = sum(commit["large"] for commit in commits)
    return {"base": base, "head": head, "max_lines": limit,
            "commits": commits, "large_commits": large,
            "total_commits": len(commits),
            "large_percent": 100 * large / len(commits) if commits else 0,
            "target_percent": 10, "errors": errors}


def main():
    policy = tomllib.loads(
        (ROOT / "tests/scripts/baselines.toml").read_text())["batches"]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default=policy["base"])
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = inspect(args.base, args.head, policy["max_lines"])
    text = json.dumps(report, indent=2) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(text)
    print(text, end="")
    return bool(report["errors"])


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("batches")
    raise SystemExit(main())
