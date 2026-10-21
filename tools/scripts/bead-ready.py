"""Practice A1: Definition of Ready for task, feature and bug beads.

A bead is ready when it has a goal (its description), at least one
acceptance criterion, a `Docs:` line naming the affected docs (or "none")
and a `Verify:` line saying how the result is checked. The lines may stand in
the description, acceptance criteria, design or notes.

    bead-ready.py ID...          check beads
    bead-ready.py --claim ID     check, then `bd update ID --claim`
    bead-ready.py --in-progress  check every claimed bead
"""
import json
import re
import subprocess
import sys

TYPES = {"task", "feature", "bug"}
FIELDS = ("description", "acceptance_criteria", "design", "notes")
LINE = {name: re.compile(rf"^[ \t]*{name}:[ \t]*\S", re.M | re.I)
        for name in ("Docs", "Verify")}
WHOLE_LINE = re.compile(r"^\s*(?:Docs|Verify):.*$", re.M | re.I)


def bd(*args):
    return json.loads(subprocess.check_output(["bd", *args, "--json"],
                                              text=True))


def errors(bead):
    """What keeps `bead` from being ready; empty when it is ready."""
    if bead.get("issue_type") not in TYPES:
        return []
    text = "\n".join(bead.get(field) or "" for field in FIELDS)
    goal = WHOLE_LINE.sub("", bead.get("description") or "").strip()
    found = [] if goal else ["no goal (empty description)"]
    if not (bead.get("acceptance_criteria") or "").strip():
        found.append("no acceptance criterion")
    found += [f"no {name}: line" for name, line in LINE.items()
              if not line.search(text)]
    return [f"{bead['id']}: {error}" for error in found]


def main(args):
    if args == ["--in-progress"]:
        beads = bd("list", "--status", "in_progress", "--limit", "0")
    elif args[:1] == ["--claim"] and len(args) == 2:
        beads = bd("show", args[1])
    elif args and not args[0].startswith("-"):
        beads = bd("show", *args)
    else:
        raise SystemExit(__doc__)
    found = [error for bead in beads for error in errors(bead)]
    if found:
        raise SystemExit("Not ready (docs/agents/beads.md#definition-of-"
                         "ready):\n" + "\n".join(found))
    if args[0] == "--claim":
        subprocess.check_call(["bd", "update", args[1], "--claim"])
    print(f"{len(beads)} beads ready")


if __name__ == "__main__":
    main(sys.argv[1:])
