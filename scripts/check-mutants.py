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


def check(outcomes, baseline):
    """The score line; fails below the baseline."""
    percentage, counts = score(outcomes)
    if percentage < baseline:
        raise ValueError(f"Mutation score {percentage}% < baseline "
                         f"{baseline}% (scripts/baselines.toml)")
    message = f"Mutation score {percentage}% (baseline {baseline}%): {counts}"
    if percentage >= baseline + 0.5:
        message += (f"\nRaise [mutants] score in scripts/baselines.toml to "
                    f"{percentage}")
    return message


if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("mutants")
    outcomes = [json.loads(Path(name).read_text()) for name in sys.argv[1:]]
    baseline = tomllib.loads(BASELINES.read_text())["mutants"]["score"]
    print(check(outcomes, baseline))
