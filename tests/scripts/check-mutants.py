"""Practice B2: the weekly mutation score must not drop below its baseline.

Reads the outcomes.json of every cargo-mutants shard. A mutant that timed out
counts as caught (the tests noticed it); unviable mutants do not count.
"""
import json
from pathlib import Path
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


def check(outcomes, baseline):
    """The score line; fails below the baseline or on an unbuildable crate."""
    if packages := unbuildable(outcomes):
        raise ValueError(
            f"Every mutant in {', '.join(packages)} is unviable: "
            "cargo-mutants builds each package alone, so check `cargo test "
            "--package <name> --all-features` (a missing feature forward?)")
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
    baseline = tomllib.loads(BASELINES.read_text())["mutants"]["score"]
    print(check(outcomes, baseline))
