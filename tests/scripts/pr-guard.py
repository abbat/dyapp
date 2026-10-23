"""Review aid for untrusted PRs; run this from the base commit."""
import datetime as dt
import json
import os
import re
import subprocess
import sys
import tomllib
from urllib.parse import quote
from urllib.request import Request, urlopen

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
CRATES_API = "https://crates.io/api/v1/crates"
GUARD_FILES = {".github/workflows/pr-guard.yml", "tests/scripts/pr-guard.py"}
INFRA_PREFIXES = (".github/", ".agents/", ".claude/", "docker/",
                  "scripts/", "tools/docker/", "tests/docker/",
                  "tools/scripts/", "tests/scripts/", "docs/agents/")
INFRA_FILES = {"Makefile", "AGENTS.md", "CLAUDE.md", "deny.toml",
               ".pre-commit-config.yaml", "tests/test_pr_guard.py"}
SHA = re.compile(r"[0-9a-f]{40}\Z")


def git(*args):
    result = subprocess.run(["git", *args], capture_output=True, check=False)
    if result.returncode:
        raise ValueError(result.stderr.decode(errors="replace").strip())
    return result.stdout


def kind(path):
    if path.startswith("docs/") and not path.startswith("docs/agents/"):
        return "docs"
    if path.endswith("Cargo.lock") or path.endswith("Cargo.toml"):
        return "deps"
    if path in INFRA_FILES or path.startswith(INFRA_PREFIXES):
        return "infra"
    if path.startswith("rust-toolchain"):
        return "infra"
    return "code"


def packages(sha, path):
    exists = subprocess.run(["git", "cat-file", "-e", f"{sha}:{path}"],
                            capture_output=True, check=False)
    if exists.returncode:
        return {}
    data = tomllib.loads(git("show", f"{sha}:{path}").decode("utf-8"))
    return {(p["name"], p["version"], p.get("source")): p
            for p in data.get("package", [])}


def published(name, version, api):
    url = f"{api.rstrip('/')}/{quote(name, safe='')}/{quote(version, safe='')}"
    request = Request(url, headers={"User-Agent": "dyapp-pr-guard/1"})
    with urlopen(request, timeout=10) as response:
        payload = response.read(1_000_001)
    if len(payload) > 1_000_000:
        raise ValueError("crates.io response too large")
    value = json.loads(payload)["version"]["created_at"]
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def check(base, head, api=None, now=None):
    api = api or os.environ.get("PR_GUARD_CRATES_API", CRATES_API)
    if not SHA.fullmatch(base) or not SHA.fullmatch(head):
        raise ValueError("base and head must be full commit hashes")
    now = now or dt.datetime.now(dt.timezone.utc)
    common = git("merge-base", base, head).decode().strip()
    diff = git("diff", "--name-only", "--no-renames", "-z", common, head)
    paths = [p.decode("utf-8") for p in diff.split(b"\0") if p]
    groups = {kind(path) for path in paths}
    errors = []
    if "infra" in groups and ("code" in groups or "deps" in groups):
        errors.append("Infrastructure changes need a separate pull request "
                      "from code and dependencies")
    warning = bool(GUARD_FILES.intersection(paths))
    changed = []
    for path in sorted(p for p in paths if p.endswith("Cargo.lock")):
        before, after = packages(common, path), packages(head, path)
        for key, package in sorted(after.items()):
            if key in before:
                if package.get("checksum") != before[key].get("checksum"):
                    errors.append(f"{path}: checksum changed without a "
                                  f"version change: {key[0]} {key[1]}")
                continue
            name, version, source = key
            changed.append(
                f"{path}: {name} {version} ({source or 'workspace'})")
            if source is None:
                continue
            if source != CRATES_IO:
                errors.append(
                    f"{path}: non-crates.io source: {name} {version}: "
                    f"{source}")
                continue
            try:
                age = now - published(name, version, api)
                if age < dt.timedelta(days=14):
                    errors.append(
                        f"{path}: {name} {version} is younger than 14 days")
            except (OSError, ValueError, KeyError,
                    json.JSONDecodeError) as error:
                errors.append(
                    f"{path}: cannot verify age of {name} {version}: {error}")
    lines = ["## Pull request guard", "", f"Changed paths: {len(paths)}.", ""]
    if warning:
        lines += ["⚠️ This PR changes the guard itself; review its workflow "
                  "and script manually.", ""]
    lines += ["### Dependency changes", ""]
    lines += [*(f"- {item}" for item in changed)] if changed else ["- None"]
    lines += ["", "### Result", ""]
    lines += ([f"- FAIL: {error}" for error in errors]
              if errors else ["- PASS"])
    summary = "\n".join(lines) + "\n"
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a",
                  encoding="utf-8") as output:
            output.write(summary)
    print(summary)
    return not errors


if __name__ == "__main__":
    try:
        success = check(*sys.argv[1:]) if len(sys.argv) == 3 else False
        if len(sys.argv) != 3:
            print("usage: pr-guard.py BASE_SHA HEAD_SHA", file=sys.stderr)
    except (OSError, ValueError, UnicodeError,
            tomllib.TOMLDecodeError) as error:
        print(f"pr-guard: {error}", file=sys.stderr)
        success = False
    sys.exit(0 if success else 1)
