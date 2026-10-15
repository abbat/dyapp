"""Serialize local Docker work; share one build budget of at most 4 CPUs."""
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import traceback
import uuid

from capture import Capture
from check_dispatch import load_runner


def run_logged(command, environment, label, cleanup=None, log_file=None):
    """Capture quiet stages with retained logs and concise diagnostics."""
    verbose = environment.get("DYAPP_VERBOSE") == "1"
    captured = output = drain = error_stream = None
    status = 0
    message = ""
    try:
        if not verbose:
            if log_file is None:
                directory = Path(environment.get(
                    "DYAPP_CHECK_LOG_DIR", "/tmp/ai/check-output"))
                directory.mkdir(parents=True, exist_ok=True)
                log_file = directory / f"docker-{uuid.uuid4().hex}.log"
            captured = Capture(log_file=log_file)
            output, drain = captured.pipe()
            error_stream = subprocess.STDOUT
        try:
            status = subprocess.run(command, env=environment, stdout=output,
                                    stderr=error_stream,
                                    check=False).returncode
        except KeyboardInterrupt:
            status = 130
        if cleanup is not None:
            result = subprocess.run(cleanup, env=environment, stdout=output,
                                    stderr=error_stream, check=False)
            if status == 0:
                status = result.returncode
    except KeyboardInterrupt:
        status = status or 130
    except OSError as error:
        message = f"{type(error).__name__}: {error}\n"
        if captured is None and verbose:
            print(message, file=sys.stderr, end="")
        elif captured is not None:
            captured.write(message.encode())
        status = status or 1
    finally:
        if output is not None:
            os.close(output)
            # ponytail: a helper that outlives the stage keeps the pipe open;
            # its later output is lost rather than waited for.
            drain.join(timeout=10)
        if captured is not None:
            captured.close()
    if not status:
        print(f"OK {label}", flush=True)
        return 0
    if verbose:
        print(f"ERROR {label} (exit {status})", file=sys.stderr, flush=True)
        return 128 - status if status < 0 else status
    text = (captured.getvalue().decode(errors="replace")
            if captured is not None else message)
    return report_failure(label, status, text,
                          log_file if captured is not None else None)


def report_failure(label, status, text, log_file=None):
    runner = load_runner()
    policy = json.loads((Path(__file__).with_name(
        "check-output.json")).read_text())["docker"]
    selected = runner.diagnostics(text, policy)
    if not selected:
        selected = runner.diagnostics(text, {"format": "python"})
    return runner.failure(label, policy["fix"], status, selected, log_file)


def config_failure(text, status, environment, log_file=None):
    """Retain preflight failures whose stdout may be a consumed JSON value."""
    log = None
    try:
        if log_file is None:
            directory = Path(environment.get(
                "DYAPP_CHECK_LOG_DIR", "/tmp/ai/check-output"))
            directory.mkdir(parents=True, exist_ok=True)
            log_file = directory / f"docker-config-{uuid.uuid4().hex}.log"
        with Path(log_file).open("a") as output:
            output.write(text)
        log = log_file
    except OSError as error:
        text += f"\n{type(error).__name__}: {error}"
    return report_failure("Docker configuration", status, text, log)


# Docker rejects a CPU limit above the host's CPU count (2 on GitHub runners).
CPUS = min(4, os.cpu_count() or 1)


def build_images(arguments, environment, log_file=None):
    value_options = {"-f", "--file", "--profile", "-p", "--project-name"}
    candidates = [i for i, value in enumerate(arguments) if value == "build"]
    index = next(i for i in candidates
                 if i == 0 or arguments[i - 1] not in value_options)
    selected = arguments[index + 1:]
    if any(name.startswith("-") for name in selected):
        return config_failure(
            "ValueError: Unsupported build options; "
            "CPU limits must be preserved",
            2, environment, log_file)
    result = subprocess.run(
        ["docker", "compose", *arguments[:index],
         "config", "--format", "json"],
        env=environment, capture_output=True, text=True, check=False,
    )
    if result.returncode:
        return config_failure(result.stdout + result.stderr,
                              result.returncode, environment, log_file)
    services = json.loads(result.stdout)["services"]
    for name in selected or list(services):
        if name not in services:
            return config_failure(f"ValueError: Unknown build service: {name}",
                                  2, environment, log_file)
        service = services[name]
        build = service.get("build")
        if not build:
            continue
        if set(build) - {"context", "dockerfile", "args", "target"}:
            return config_failure(
                f"ValueError: Unsupported build configuration: {name}",
                2, environment, log_file)
        filename = build.get("dockerfile", "Dockerfile")
        dockerfile = Path(build["context"]) / filename
        # CI layer cache (DYAPP_BUILD_CACHE=gha) needs BuildKit, which has no
        # CPU quota and cannot see local base images such as dyapp:dev-*.
        cache = environment.get("DYAPP_BUILD_CACHE")
        if cache and "FROM dyapp:" not in dockerfile.read_text():
            scope = service["image"].replace(":", "-")
            command = ["docker", "buildx", "build", "--load",
                       "--cache-from", f"type={cache},scope={scope}",
                       "--cache-to", f"type={cache},scope={scope}"]
            buildkit = "1"
        else:
            command = ["docker", "build", "--cpu-period", "100000",
                       "--cpu-quota", str(CPUS * 100000)]
            buildkit = "0"
        command.extend(["-t", service["image"], "-f", str(dockerfile)])
        if build.get("target"):
            command.extend(["--target", build["target"]])
        for key, value in build.get("args", {}).items():
            command.extend(["--build-arg", key if value is None
                            else f"{key}={value}"])
        command.append(build["context"])
        status = run_logged(
            command, {**environment, "DOCKER_BUILDKIT": buildkit},
            service["image"], log_file=log_file,
        )
        if status:
            return status
    return 0


def main():
    temporary = Path("/tmp/ai")
    temporary.mkdir(parents=True, exist_ok=True)
    # compose.dev.yml mounts a volume at /workspace/target inside the
    # read-only checkout; Docker cannot create that mountpoint itself.
    (Path(__file__).resolve().parents[1] / "target").mkdir(exist_ok=True)
    arguments = sys.argv[1:]
    log_file = None
    if arguments[:1] == ["--log-file"]:
        log_file, arguments = arguments[1], arguments[2:]
    environment = {**os.environ, "COMPOSE_PARALLEL_LIMIT": "1",
                   "COVERAGE_RUN_ID": uuid.uuid4().hex,
                   "DYAPP_CPUS": str(CPUS),
                   "DYAPP_EMULATOR_CPUS": str(max(1, CPUS - 1))}
    with (temporary / "dyapp-docker.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if "build" in arguments and "run" not in arguments:
            return build_images(arguments, environment, log_file)
        cleanup = None
        if "run" in arguments:
            prefix = arguments[:arguments.index("run")]
            cleanup = ["docker", "compose", *prefix, "stop"]
            # Captured checks must not allocate a pseudo-terminal.
            if environment.get("DYAPP_VERBOSE") != "1":
                index = arguments.index("run") + 1
                arguments.insert(index, "--no-TTY")
        label = "Docker Compose"
        if "--profile" in arguments:
            label += f" {arguments[arguments.index('--profile') + 1]}"
        return run_logged(["docker", "compose", *arguments], environment,
                          label, cleanup, log_file)


if __name__ == "__main__":
    try:
        status = main()
    except KeyboardInterrupt:
        status = config_failure("KeyboardInterrupt: interrupted", 130,
                                os.environ)
    except Exception:
        status = config_failure(traceback.format_exc(), 1, os.environ)
    sys.exit(status)
