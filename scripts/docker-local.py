"""Serialize local Docker work; share one build budget of at most 4 CPUs."""
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid

# Docker rejects a CPU limit above the host's CPU count (2 on GitHub runners).
CPUS = min(4, os.cpu_count() or 1)


def build_images(arguments, environment):
    value_options = {"-f", "--file", "--profile", "-p", "--project-name"}
    candidates = [i for i, value in enumerate(arguments) if value == "build"]
    index = next(i for i in candidates
                 if i == 0 or arguments[i - 1] not in value_options)
    selected = arguments[index + 1:]
    if any(name.startswith("-") for name in selected):
        print("Unsupported build options; CPU limits must be preserved.",
              file=sys.stderr)
        return 2
    result = subprocess.run(
        ["docker", "compose", *arguments[:index],
         "config", "--format", "json"],
        env=environment, capture_output=True, text=True, check=False,
    )
    if result.returncode:
        print(result.stderr, file=sys.stderr)
        return result.returncode
    services = json.loads(result.stdout)["services"]
    for name in selected or list(services):
        if name not in services:
            print(f"Unknown build service: {name}", file=sys.stderr)
            return 2
        service = services[name]
        build = service.get("build")
        if not build:
            continue
        if set(build) - {"context", "dockerfile", "args", "target"}:
            print(f"Unsupported build configuration: {name}", file=sys.stderr)
            return 2
        filename = build.get("dockerfile", "Dockerfile")
        dockerfile = Path(build["context"]) / filename
        command = ["docker", "build", "--cpu-period", "100000",
                   "--cpu-quota", str(CPUS * 100000),
                   "-t", service["image"],
                   "-f", str(dockerfile)]
        if build.get("target"):
            command.extend(["--target", build["target"]])
        for key, value in build.get("args", {}).items():
            command.extend(["--build-arg", key if value is None
                            else f"{key}={value}"])
        command.append(build["context"])
        # Build logs are long; show them only when the build fails.
        result = subprocess.run(
            command, env={**environment, "DOCKER_BUILDKIT": "0"},
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
        )
        if result.returncode:
            sys.stdout.flush()
            sys.stdout.buffer.write(result.stdout)
            print(f"FAILED {service['image']} (exit {result.returncode})",
                  file=sys.stderr)
            return result.returncode
        print(f"OK {service['image']}")
    return 0


def main():
    temporary = Path("/tmp/ai")
    temporary.mkdir(parents=True, exist_ok=True)
    arguments = sys.argv[1:]
    environment = {**os.environ, "COMPOSE_PARALLEL_LIMIT": "1",
                   "COVERAGE_RUN_ID": uuid.uuid4().hex,
                   "DYAPP_CPUS": str(CPUS),
                   "DYAPP_EMULATOR_CPUS": str(max(1, CPUS - 1))}
    with (temporary / "dyapp-docker.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            if "build" in arguments and "run" not in arguments:
                return build_images(arguments, environment)
            result = subprocess.run(["docker", "compose", *arguments],
                                    env=environment, check=False)
            status = result.returncode
        except KeyboardInterrupt:
            status = 130
        if "run" in arguments:
            prefix = arguments[:arguments.index("run")]
            cleanup = subprocess.run(["docker", "compose", *prefix, "stop"],
                                     env=environment, check=False)
            if status == 0:
                status = cleanup.returncode
        return status


if __name__ == "__main__":
    sys.exit(main())
