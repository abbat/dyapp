"""Retrieve Docker coverage artifacts; the host only copies files."""
import json
from pathlib import Path
import subprocess
import sys

output = subprocess.check_output([
    "docker", "ps", "--all", "--quiet",
    "--filter", "label=com.docker.compose.project=dyapp",
    "--filter", "label=com.docker.compose.service=coverage",
], text=True).splitlines()
if not output:
    sys.exit("No coverage container; report export failed")
container = json.loads(subprocess.check_output(
    ["docker", "inspect", output[0]], text=True))[0]
variables = dict(item.split("=", 1) for item in container["Config"]["Env"])
run_id = variables.get("COVERAGE_RUN_ID", "")
if len(run_id) != 32 or any(c not in "0123456789abcdef" for c in run_id):
    sys.exit("Coverage container has no valid run ID")
source = f"{output[0]}:/workspace/target/coverage.{run_id}"
destination = Path(sys.argv[1])
destination.mkdir(parents=True, exist_ok=True)
for extension in ("txt", "json", "lcov"):
    filename = f"coverage.{extension}"
    subprocess.run([
        "docker", "cp", f"{source}/{filename}",
        str(destination / filename),
    ], check=True)
