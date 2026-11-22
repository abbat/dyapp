"""Validate a trusted main CI producer before installing its APK artifact."""

import argparse
import json
from pathlib import Path
import re
from typing import cast


def mapping(value: object) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError("Expected a JSON object")
    return cast(dict[str, object], value)


def validate(
    run: dict[str, object],
    jobs: list[dict[str, object]],
    artifacts: list[dict[str, object]],
    repository: str,
    run_id: int,
) -> dict[str, str]:
    source = str(run.get("head_sha", ""))
    repo = mapping(run.get("repository"))
    head = mapping(run.get("head_repository"))
    if (
        run.get("id") != run_id
        or run.get("path") != ".github/workflows/ci.yml"
        or run.get("event") not in ("push", "workflow_dispatch")
        or run.get("head_branch") != "main"
        or repo.get("full_name") != repository
        or head.get("full_name") != repository
        or repo.get("id") != head.get("id")
        or not isinstance(repo.get("id"), int)
        or re.fullmatch(r"[0-9a-f]{40}", source) is None
    ):
        raise ValueError("Untrusted Android producer")
    builds = [job for job in jobs if job.get("name") == "docker - android"]
    if (
        len(builds) != 1
        or builds[0].get("status") != "completed"
        or builds[0].get("conclusion") != "success"
        or builds[0].get("head_sha") != source
        or builds[0].get("run_id") != run_id
    ):
        raise ValueError("Exact-source Android build has not succeeded")
    packages = [
        item
        for item in artifacts
        if item.get("name") == "android-runtime-packages"
    ]
    if len(packages) != 1:
        raise ValueError("Expected exactly one Android runtime artifact")
    artifact = packages[0]
    producer = mapping(artifact.get("workflow_run"))
    artifact_id = artifact.get("id")
    digest = str(artifact.get("digest", ""))
    if (
        artifact.get("expired") is not False
        or not isinstance(artifact_id, int)
        or artifact_id <= 0
        or producer.get("id") != run_id
        or producer.get("head_sha") != source
        or producer.get("head_branch") != "main"
        or producer.get("head_repository_id") != repo["id"]
        or re.fullmatch(r"sha256:[0-9a-f]{64}", digest) is None
    ):
        raise ValueError("Android artifact provenance mismatch")
    return {
        "source_sha": source,
        "producer_run": str(run_id),
        "artifact_id": str(artifact_id),
        "artifact_digest": digest,
    }


def pages(path: Path, key: str) -> list[dict[str, object]]:
    data = json.loads(path.read_text())
    if not isinstance(data, list):
        raise ValueError("Expected paginated API responses")
    result: list[dict[str, object]] = []
    for page in data:
        entries = mapping(page).get(key)
        if not isinstance(entries, list):
            raise ValueError("Expected API page entries")
        result.extend(mapping(entry) for entry in entries)
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--run", required=True)
    args = parser.parse_args()
    if re.fullmatch(r"[1-9][0-9]{0,19}", args.run) is None:
        parser.error("Producer run must be a positive numeric ID")
    directory = args.directory
    result = validate(
        mapping(json.loads((directory / "producer.json").read_text())),
        pages(directory / "jobs.json", "jobs"),
        pages(directory / "artifacts.json", "artifacts"),
        args.repository,
        int(args.run),
    )
    (directory / "validated.json").write_text(
        json.dumps(result, indent=2) + "\n"
    )
    for key, value in result.items():
        print(f"{key}={value}")


if __name__ == "__main__":
    main()
