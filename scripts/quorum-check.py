"""A6 local delivery gate: bind a closed quorum to the exact reviewed range."""
import argparse
import json
from pathlib import Path
import re
import subprocess

import triggers

ROOT = Path(__file__).resolve().parents[1]


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def git(*args):
    return command("git", "-c", f"safe.directory={ROOT}", *args)


def proof_errors(quorum, participants, base, head):
    errors = []
    if any((quorum["issue_type"] != "epic", quorum["status"] != "closed",
            "quorum" not in quorum.get("labels", []))):
        errors.append("Review must be a closed quorum epic")
    description = quorum.get("description") or ""
    for field, sha in [("Snapshot", head), ("Diff base", base)]:
        match = re.search(rf"^{field}:\s*([0-9a-f]{{40}})\s*$",
                          description, re.M)
        if not match or match[1] != sha:
            errors.append(f"Quorum {field} does not match the reviewed range")
    members = [p for p in participants if all((
        p["issue_type"] == "epic", "quorum" in p.get("labels", []),
        not (p.get("close_reason") or "").startswith("withdrawn:")))]
    names = {p.get("assignee") for p in members if p.get("assignee")}
    if any((len(names) < 2,
            any(p["status"] != "closed" for p in members),
            any(not p.get("assignee") for p in members))):
        errors.append("At least two distinct closed participants are required")
    return errors


def needs_review(hits, tightening, protected):
    # A6a reports all check changes; A6 only gates weakening, not tightening.
    security = any(not hit.startswith("checks: ") for hit in hits)
    return bool(security or protected or (hits and not tightening))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--quorum", help="private local quorum epic ID")
    parser.add_argument("--checks-tightening", action="store_true",
                        help="attest check-only changes do not weaken gates")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = {"errors": []}
    try:
        if git("status", "--porcelain").strip():
            raise ValueError("Delivery requires a clean working tree")
        base = git("rev-parse", "--verify", args.base + "^{commit}").strip()
        head = git("rev-parse", "--verify", args.head + "^{commit}").strip()
        git("merge-base", "--is-ancestor", base, head)
        diff = git("-c", "core.quotePath=false", "diff", "--unified=0",
                   "--no-renames", "--no-color", "--no-ext-diff",
                   "--no-textconv", base, head)
        files = triggers.changes(diff)
        policies = [triggers.load(git("show", f"{sha}:scripts/triggers.toml"))
                    for sha in (base, head)]
        hits = sorted({hit for policy in policies
                       for hit in triggers.fired(policy, files)})
        trailers = git("log", "--format=%(trailers)", base + ".." + head)
        hatch = bool(re.search(
            r"^(Large-Change|Size-Increase|Docs-Unchanged):\s*\S",
            trailers, re.M))
        hatch = hatch or bool(re.search(
            r"^\+\s*(#\[mutants::skip|(?://|#).*gitleaks:allow)", diff, re.M))
        protected = bool(re.search(
            r"^Quorum:\s*not-needed\s*$", trailers, re.M))
        protected = protected or (hatch and bool(hits))
        required = needs_review(hits, args.checks_tightening, protected)
        result.update({"base": base, "head": head, "triggers": hits,
                       "quorum_required": required,
                       "checks_tightening": args.checks_tightening})
        if required:
            if not args.quorum:
                result["errors"].append("A closed quorum is required")
            else:
                quorum = json.loads(command("bd", "show", args.quorum,
                                            "--json"))[0]
                participants = json.loads(command(
                    "bd", "list", "--parent", args.quorum, "--all",
                    "--limit", "0", "--brief", "--json"))
                errors = proof_errors(quorum, participants, base, head)
                result["errors"] += errors
    except (ValueError, KeyError, TypeError, OSError,
            subprocess.CalledProcessError) as error:
        result["errors"].append(str(error))
    text = json.dumps(result, indent=2) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(text)
    print(text, end="")
    return bool(result["errors"])


if __name__ == "__main__":
    raise SystemExit(main())
