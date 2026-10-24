"""Practice B2: the weekly mutation score must not drop below its baseline.

Reads the outcomes.json of every cargo-mutants shard. A mutant that timed out
counts as caught (the tests noticed it); unviable mutants do not count.
Every mutant in the [mutants.invariants] scope must be caught.
"""
import json
from pathlib import Path
import re
import sys
import tomllib

BASELINES = Path(__file__).with_name("baselines.toml")


def score(outcomes):
    """Mutation score in percent over all shards, and the counts."""
    counts = {key: sum(item[key] for item in outcomes)
              for key in ("caught", "timeout", "missed")}
    caught = counts["caught"] + counts["timeout"]
    total = caught + counts["missed"]
    if not total:
        raise ValueError("No viable mutants were tested")
    return round(100 * caught / total, 1), counts


def unbuildable(outcomes):
    """Packages none of whose mutants built: likely it fails to build alone."""
    viable = {}
    for item in outcomes:
        for outcome in item.get("outcomes", []):
            # The unmutated baseline's scenario is the string "Baseline".
            if isinstance(outcome["scenario"], dict):
                package = outcome["scenario"]["Mutant"]["package"]
                built = outcome["summary"] != "Unviable"
                viable[package] = viable.get(package, False) or built
    return sorted(package for package, ok in viable.items() if not ok)


def invariants(outcomes, scope):
    """Fails on a missed mutant in scope, or a scope entry no mutant matched.

    scope maps a file to a regex searched in the mutated function's name.
    """
    missed, matched = [], set()
    for item in outcomes:
        for outcome in item.get("outcomes", []):
            if not isinstance(outcome["scenario"], dict):
                continue
            mutant = outcome["scenario"]["Mutant"]
            pattern = scope.get(mutant["file"])
            function = (mutant.get("function") or {}).get("function_name")
            if pattern is None or function is None \
                    or not re.search(pattern, function):
                continue
            matched.add(mutant["file"])
            if outcome["summary"] == "MissedMutant":
                missed.append(mutant["name"])
    if unmatched := sorted(set(scope) - matched):
        names = ", ".join(unmatched)
        raise ValueError(
            f"[mutants.invariants] entries match no mutant: {names}")
    if missed:
        names = "\n".join(sorted(missed))
        raise ValueError(f"Missed security-invariant mutants:\n{names}")


def check(outcomes, baseline, scope=None):
    """The score line; fails below the baseline, on an unbuildable crate, or
    on a missed mutant in the security-invariant scope."""
    if packages := unbuildable(outcomes):
        raise ValueError(
            f"Every mutant in {', '.join(packages)} is unviable: "
            "cargo-mutants builds each package alone, so check `cargo test "
            "--package <name> --all-features` (a missing feature forward?)")
    if scope:
        invariants(outcomes, scope)
    percentage, counts = score(outcomes)
    if percentage < baseline:
        raise ValueError(f"Mutation score {percentage}% < baseline "
                         f"{baseline}% (tests/scripts/baselines.toml)")
    message = f"Mutation score {percentage}% (baseline {baseline}%): {counts}"
    if percentage >= baseline + 0.5:
        message += ("\nRaise [mutants] score in "
                    "tests/scripts/baselines.toml to "
                    f"{percentage}")
    return message


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("mutants")
    outcomes = [json.loads(Path(name).read_text()) for name in sys.argv[1:]]
    mutants = tomllib.loads(BASELINES.read_text())["mutants"]
    print(check(outcomes, mutants["score"], mutants.get("invariants")))
