"""Practice A5: commits that trigger an ADR name one in an `ADR:` trailer.

Triggers, per commit after BASE:
- a new crate: an added Cargo.toml with [package];
- a new direct dependency: a new name in a Cargo.toml dependency table (path
  dependencies on our own crates and `workspace = true` reuses excepted) or a
  new Gradle coordinate;
- a wire-format change: any change to proto/*.proto;
- a storage-schema change: an added or removed SQL schema line in Rust code.

The trailer `ADR: docs/decisions/NNNN-title.md` must name an ADR that exists
in that commit; an older ADR that covers the change is fine.
"""
from pathlib import PurePosixPath
import re
import subprocess
import tomllib

# Practice A5 adoption: only later commits are checked.
BASE = "96384248c85b3e11784fe5f443ee6876de4629bd"
TRAILER = re.compile(r"^ADR: (docs/decisions/\d{4}-[\w.-]+\.md)$", re.M)
GRADLE = re.compile(r"^\s*\w*[iI]mplementation\s*\(?\s*(?:platform\()?\s*"
                    r"""['"]([\w.-]+:[\w.-]+)""", re.M)
SCHEMA = re.compile(r"\b(?:CREATE|ALTER|DROP) (?:TABLE|INDEX)\b"
                    r"|PRAGMA user_version|`\w+` (?:BLOB|INTEGER|TEXT|REAL)\b")
# Pushed commits that lack the trailer and the ADR covering each;
# history is not rewritten.
COVERED = {
    "792eb488d0f521f4e99647b0eed6dbb0ea1b3efe":
        "docs/decisions/0015-sqlite-node-storage.md",
    "b665f8eb6103cfae36c96750ab1b7e62216ece23":
        "docs/decisions/0014-libp2p-only-node-protocol.md",
    "47d3207e88219a2eb6619f108aa43fd307a70804":
        "docs/decisions/0016-protocol-evolution.md",
    "491a1d7a03ab0dd8e4ef3bb763ee21a580525e88":
        "docs/decisions/0016-protocol-evolution.md",
    "ea10ace72a43d1ef28acc5a3558d66efa3cb4818":
        "docs/decisions/0015-sqlite-node-storage.md",
    "6715ab5de96764ca489818ac6ef2e89e9be927cc":
        "docs/decisions/0016-protocol-evolution.md",
    "a2ba459e70cef16fcbaf1d2aef1c26eb76d27bd6":
        "docs/decisions/0009-message-delivery-and-storage.md",
    "bc6cee52f9930c5637f34117847045ea18762cc0":
        "docs/decisions/0009-message-delivery-and-storage.md",
    "9f41870bd54c9c7d78fec0455766395b70a8c735":
        "docs/decisions/0009-message-delivery-and-storage.md",
    "94776c519116605dd2dd43c63d28eec1489803b0":
        "docs/decisions/0009-message-delivery-and-storage.md",
}
GIT = ["git", "-c", "safe.directory=*"]
WATCHED = ("Cargo.toml", ".gradle", ".kts", ".proto", ".rs")


def dependencies(text):
    """Names in every dependency table of a Cargo.toml but path and
    workspace ones."""
    data = tomllib.loads(text) if text else {}
    tables = [data, data.get("workspace", {}),
              *data.get("target", {}).values()]
    specs = [(name, spec if isinstance(spec, dict) else {})
             for table in tables
             for kind in ("dependencies", "dev-dependencies",
                          "build-dependencies")
             for name, spec in table.get(kind, {}).items()]
    return {name for name, spec in specs
            if "path" not in spec and not spec.get("workspace")}


def triggers(path, old, new, diff):
    """Why a change of `path` from `old` to `new` text needs an ADR."""
    name = PurePosixPath(path)
    found = []
    if name.name == "Cargo.toml":
        if not old and "[package]" in new:
            found.append(f"new crate {path}")
        found += [f"new dependency {dep} in {path}"
                  for dep in sorted(dependencies(new) - dependencies(old))]
    elif name.suffix in (".gradle", ".kts"):
        found += [f"new dependency {dep} in {path}" for dep in sorted(
            set(GRADLE.findall(new)) - set(GRADLE.findall(old)))]
    elif name.match("proto/*.proto"):
        found.append(f"wire format {path}")
    elif name.suffix == ".rs" and any(
            line[:1] in "+-" and SCHEMA.search(line)
            for line in diff.splitlines()
            if not line.startswith(("+++", "---"))):
        found.append(f"storage schema {path}")
    return found


def show(sha, path):
    result = subprocess.run(GIT + ["show", f"{sha}:{path}"],
                            capture_output=True, check=False)
    return result.stdout.decode(errors="replace") \
        if result.returncode == 0 else ""


def commit_errors(sha):
    message = subprocess.check_output(
        GIT + ["log", "-1", "--format=%B", sha], text=True)
    paths = subprocess.check_output(
        GIT + ["diff-tree", "--root", "--no-commit-id", "-r", "--name-only",
               sha], text=True).split()
    found = []
    for path in paths:
        if not path.endswith(WATCHED):
            continue
        diff = subprocess.check_output(
            GIT + ["diff", "-U0", f"{sha}^!", "--", path], text=True) \
            if path.endswith(".rs") else ""
        found += triggers(path, show(f"{sha}^", path), show(sha, path), diff)
    if not found:
        return []
    if sha in COVERED:
        return []
    adr = TRAILER.search(message)
    if adr and show(sha, adr.group(1)):
        return []
    reason = "names a missing ADR" if adr else "has no ADR: trailer"
    return [f"{sha[:12]} {reason}: " + "; ".join(found)]


if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("adr")
    subprocess.check_call(GIT + ["cat-file", "-e", BASE + "^{commit}"])
    shas = subprocess.check_output(
        GIT + ["rev-list", "--no-merges", "--reverse", BASE + "..HEAD"],
        text=True).split()
    errors = [error for sha in shas for error in commit_errors(sha)]
    if errors:
        errors.insert(0, "ADR triggers (docs/decisions/README.md#triggers):")
        raise ValueError("\n".join(errors))
    print(f"{len(shas)} commits: every ADR trigger names an ADR")
