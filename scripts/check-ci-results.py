"""Require every expected platform CI job to complete successfully."""
import argparse
import json
import os

REQUIRED = {"core", "docker", "linux", "apple", "windows"}


def validate(results):
    if not isinstance(results, dict) or not results:
        raise ValueError("Missing or invalid CI job inventory")
    errors = [f"{name}: missing" for name in sorted(REQUIRED - results.keys())]
    for name, value in sorted(results.items()):
        if not isinstance(value, dict) or value.get("result") != "success":
            state = value.get("result") if isinstance(value, dict) else value
            errors.append(f"{name}: {state!r}")
    if errors:
        raise ValueError("CI jobs not successful: " + "; ".join(errors))


if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("ci-results")
    argparse.ArgumentParser(description=__doc__).parse_args()
    validate(json.loads(os.environ["RESULTS"]))
    print("All required CI jobs passed")
