"""Serialize local Docker work; share one build budget of at most 4 CPUs."""

from collections.abc import Iterator, Mapping, Sequence
from contextlib import contextmanager
import fcntl
import json
import os
import re
import shutil
from pathlib import Path
import subprocess
import sys
import traceback
from threading import Thread
from typing import NotRequired, TypedDict
import uuid

from capture import Capture
from check_dispatch import CheckPolicy, load_runner
from docker_build_cache import cache_recovery_command, read_build_log


class ComposeBuild(TypedDict):
    context: str
    dockerfile: NotRequired[str]
    target: NotRequired[str]
    args: NotRequired[dict[str, str | int | float | bool | None]]


class ComposeService(TypedDict):
    image: str
    build: NotRequired[ComposeBuild]


def stage_result(
    status: int,
    label: str,
    verbose: bool,
    captured: Capture | None,
    message: str,
    log_file: str | os.PathLike[str] | None,
    quiet_failure: bool = False,
) -> int:
    if status and quiet_failure and captured is not None:
        return 128 - status if status < 0 else status
    if not status:
        print(f"OK {label}", flush=True)
        return 0
    if verbose:
        print(f"ERROR {label} (exit {status})", file=sys.stderr, flush=True)
        return 128 - status if status < 0 else status
    text = (
        captured.getvalue().decode(errors="replace")
        if captured is not None
        else message
    )
    return report_failure(
        label, status, text, log_file if captured is not None else None
    )


def close_capture(
    output: int | None, drain: Thread | None, captured: Capture | None
) -> None:
    if output is not None:
        os.close(output)
        # A helper that outlives the stage can keep the pipe open.
        assert drain is not None
        drain.join(timeout=10)
    if captured is not None:
        captured.close()


def startup_error(
    captured: Capture | None, verbose: bool, message: str
) -> None:
    if captured is None and verbose:
        print(message, file=sys.stderr, end="")
    elif captured is not None:
        captured.write(message.encode())


def run_logged(
    command: Sequence[str],
    environment: Mapping[str, str],
    label: str,
    cleanup: Sequence[str] | None = None,
    log_file: str | os.PathLike[str] | None = None,
    quiet_failure: bool = False,
) -> int:
    """Capture quiet stages with retained logs and concise diagnostics."""
    verbose = environment.get("DYAPP_VERBOSE") == "1"
    captured: Capture | None = None
    output: int | None = None
    drain: Thread | None = None
    error_stream: int | None = None
    status = 0
    message = ""
    try:
        if not verbose:
            if log_file is None:
                directory = Path(
                    environment.get(
                        "DYAPP_CHECK_LOG_DIR", "/tmp/ai/check-output"
                    )
                )
                directory.mkdir(parents=True, exist_ok=True)
                log_file = directory / f"docker-{uuid.uuid4().hex}.log"
            captured = Capture(log_file=log_file)
            output, drain = captured.pipe()
            error_stream = subprocess.STDOUT
        try:
            status = subprocess.run(
                command,
                env=environment,
                stdout=output,
                stderr=error_stream,
                check=False,
            ).returncode
        except KeyboardInterrupt:
            status = 130
        if cleanup is not None:
            result = subprocess.run(
                cleanup,
                env=environment,
                stdout=output,
                stderr=error_stream,
                check=False,
            )
            if status == 0:
                status = result.returncode
    except KeyboardInterrupt:
        status = status or 130
    except OSError as error:
        message = f"{type(error).__name__}: {error}\n"
        startup_error(captured, verbose, message)
        status = status or 1
    finally:
        close_capture(output, drain, captured)
    return stage_result(
        status, label, verbose, captured, message, log_file, quiet_failure
    )


def report_failure(
    label: str,
    status: int,
    text: str,
    log_file: str | os.PathLike[str] | None = None,
) -> int:
    runner = load_runner()
    policy: CheckPolicy = json.loads(
        (Path(__file__).with_name("check-output.json")).read_text()
    )["docker"]
    selected = runner.diagnostics(text, policy)
    if not selected:
        selected = runner.diagnostics(text, {"format": "python"})
    return runner.failure(label, policy["fix"], status, selected, log_file)


def config_failure(
    text: str,
    status: int,
    environment: Mapping[str, str],
    log_file: str | os.PathLike[str] | None = None,
) -> int:
    """Retain preflight failures whose stdout may be a consumed JSON value."""
    log = None
    try:
        if log_file is None:
            directory = Path(
                environment.get("DYAPP_CHECK_LOG_DIR", "/tmp/ai/check-output")
            )
            directory.mkdir(parents=True, exist_ok=True)
            log_file = directory / f"docker-config-{uuid.uuid4().hex}.log"
        with Path(log_file).open("a") as output:
            output.write(text)
        log = log_file
    except OSError as error:
        text += f"\n{type(error).__name__}: {error}"
    return report_failure("Docker configuration", status, text, log)


# Docker rejects a CPU limit above the host's CPU count (2 on GitHub
# runners); DYAPP_CPUS lowers it.
CPUS = min(int(os.environ.get("DYAPP_CPUS") or 4), 4, os.cpu_count() or 1)


def image_build_command(
    build: ComposeBuild,
    service: ComposeService,
    environment: Mapping[str, str],
) -> tuple[list[str], str]:
    filename = build.get("dockerfile", "Dockerfile")
    dockerfile = Path(build["context"]) / filename
    # CI layer cache (DYAPP_BUILD_CACHE=gha) needs BuildKit, which has no
    # CPU quota and cannot see local base images such as dyapp:dev-*.
    cache = environment.get("DYAPP_BUILD_CACHE")
    if cache and "FROM dyapp:" not in dockerfile.read_text():
        scope = service["image"].replace(":", "-")
        command = [
            "docker",
            "buildx",
            "build",
            "--load",
            "--cache-from",
            f"type={cache},scope={scope}",
        ]
        # Only main writes the cache: other branches and pull requests
        # read it, so they neither poison it nor fill its 10 GB limit.
        if environment.get("GITHUB_REF") == "refs/heads/main":
            command += ["--cache-to", f"type={cache},scope={scope}"]
        buildkit = "1"
    else:
        command = [
            "docker",
            "build",
            "--cpu-period",
            "100000",
            "--cpu-quota",
            str(CPUS * 100000),
            "--build-arg",
            f"DYAPP_CPUS={CPUS}",
        ]
        buildkit = "0"
    command.extend(
        [
            "--label",
            "dyapp.local",
            "-t",
            service["image"],
            "-f",
            str(dockerfile),
        ]
    )
    if build.get("target"):
        command.extend(["--target", build["target"]])
    for key, value in build.get("args", {}).items():
        command.extend(
            ["--build-arg", key if value is None else f"{key}={value}"]
        )
    command.append(build["context"])
    return command, buildkit


def build_image(
    command: Sequence[str],
    environment: Mapping[str, str],
    label: str,
    log_file: str | os.PathLike[str] | None = None,
) -> int:
    if (
        "--cache-from" not in command
        or environment.get("DYAPP_VERBOSE") == "1"
    ):
        return run_logged(command, environment, label, log_file=log_file)
    directory = Path(
        environment.get("DYAPP_CHECK_LOG_DIR", "build/check-output")
    )
    directory.mkdir(parents=True, exist_ok=True)
    first = (
        Path(log_file)
        if log_file
        else directory / f"docker-{uuid.uuid4().hex}.log"
    )
    status = run_logged(
        command, environment, label, log_file=first, quiet_failure=True
    )
    if not status or not first.is_file():
        return status
    text = read_build_log(first)
    fresh = cache_recovery_command(command, status, text)
    if fresh is None:
        return report_failure(label, status, text, first)
    if log_file is not None:
        retained = directory / f"docker-cache-failure-{uuid.uuid4().hex}.log"
        shutil.copyfile(first, retained)
        first = retained
    recovery = directory / f"docker-cache-recovery-{uuid.uuid4().hex}.log"
    print(
        f"INFO {label}: rebuilding without remote cache; first log: {first}",
        flush=True,
    )
    return run_logged(fresh, environment, label, log_file=recovery)


def build_images(
    arguments: Sequence[str],
    environment: Mapping[str, str],
    log_file: str | os.PathLike[str] | None = None,
) -> int:
    value_options = {"-f", "--file", "--profile", "-p", "--project-name"}
    candidates = [i for i, value in enumerate(arguments) if value == "build"]
    index = next(
        i
        for i in candidates
        if i == 0 or arguments[i - 1] not in value_options
    )
    selected = arguments[index + 1 :]
    if any(name.startswith("-") for name in selected):
        return config_failure(
            "ValueError: Unsupported build options; "
            "CPU limits must be preserved",
            2,
            environment,
            log_file,
        )
    result = subprocess.run(
        [
            "docker",
            "compose",
            *arguments[:index],
            "config",
            "--format",
            "json",
        ],
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode:
        return config_failure(
            result.stdout + result.stderr,
            result.returncode,
            environment,
            log_file,
        )
    services: dict[str, ComposeService] = json.loads(result.stdout)["services"]
    for name in selected or list(services):
        if name not in services:
            return config_failure(
                f"ValueError: Unknown build service: {name}",
                2,
                environment,
                log_file,
            )
        service = services[name]
        build = service.get("build")
        if not build:
            continue
        if set(build) - {"context", "dockerfile", "args", "target"}:
            return config_failure(
                f"ValueError: Unsupported build configuration: {name}",
                2,
                environment,
                log_file,
            )
        command, buildkit = image_build_command(build, service, environment)
        status = build_image(
            command,
            {**environment, "DOCKER_BUILDKIT": buildkit},
            service["image"],
            log_file=log_file,
        )
        if status:
            return status
    # A rebuilt image leaves its old version untagged; base images pulled
    # by digest lack the label.
    subprocess.run(
        ["docker", "image", "prune", "-f", "--filter", "label=dyapp.local"],
        env=environment,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return 0


def make_bind_directories(
    arguments: Sequence[str], root: Path | None = None
) -> None:
    """Create the build/ bind mounts of the given compose files.

    So build/ can be deleted. Made here, they are the host user's; Docker
    would create a missing one as root.
    """
    root = root or Path(__file__).resolve().parents[2]
    files = [
        arguments[i + 1]
        for i, value in enumerate(arguments[:-1])
        if value in ("-f", "--file")
    ]
    for file in files:
        text = (root / file).read_text()
        for name in re.findall(r"\.\./\.\./(build/[^:\s]+):", text):
            directory = root / name
            directory.mkdir(parents=True, exist_ok=True)


def compose_files(root: Path, arguments: Sequence[str]) -> list[Path]:
    return [
        root / arguments[i + 1]
        for i, value in enumerate(arguments[:-1])
        if value in ("-f", "--file")
    ]


def compose_services(root: Path, arguments: Sequence[str]) -> set[str]:
    services = set()
    for file in compose_files(root, arguments):
        for line in file.read_text().splitlines():
            match = re.match(r"^  ([a-zA-Z0-9_-]+):$", line)
            if match:
                services.add(match.group(1))
    return services


def worktree_git_override(root: Path, services: set[str]) -> str | None:
    """Mount main-checkout git metadata at the path a worktree references."""
    marker = root / ".git"
    if not marker.is_file():
        return None
    text = marker.read_text().strip()
    if not text.startswith("gitdir:"):
        return None
    gitdir = Path(text.removeprefix("gitdir:").strip())
    if not gitdir.is_absolute():
        gitdir = (root / gitdir).resolve()
    if (
        gitdir.parent.name != "worktrees"
        or gitdir.parent.parent.name != ".git"
    ):
        return None
    common = gitdir.parent.parent
    selected = sorted(services & {"dev", "quality", "coverage", "security"})
    if not selected:
        return None
    mount = json.dumps(f"{common}:{common}:ro")
    return "services:\n" + "".join(
        f"  {service}:\n    volumes: [{mount}]\n" for service in selected
    )


@contextmanager
def worktree_arguments(
    root: Path, arguments: list[str], temporary: Path
) -> Iterator[list[str]]:
    text = worktree_git_override(root, compose_services(root, arguments))
    if text is None:
        yield arguments
        return
    override = temporary / f"dyapp-worktree-{uuid.uuid4().hex}.yml"
    override.write_text(text)
    try:
        index = next(
            (
                i
                for i, argument in enumerate(arguments)
                if argument in ("build", "run")
            ),
            len(arguments),
        )
        yield [*arguments[:index], "-f", str(override), *arguments[index:]]
    finally:
        override.unlink(missing_ok=True)


def main() -> int:
    temporary = Path("/tmp/ai")
    temporary.mkdir(parents=True, exist_ok=True)
    arguments = sys.argv[1:]
    log_file: str | None = None
    if arguments[:1] == ["--log-file"]:
        log_file, arguments = arguments[1], arguments[2:]
    make_bind_directories(arguments)
    uid, gid = str(os.getuid()), str(os.getgid())
    environment = {
        **os.environ,
        "COMPOSE_PARALLEL_LIMIT": "1",
        "COVERAGE_RUN_ID": uuid.uuid4().hex,
        "DYAPP_CPUS": str(CPUS),
        "DYAPP_UID": os.environ.get("DYAPP_UID", "") or uid,
        "DYAPP_GID": os.environ.get("DYAPP_GID", "") or gid,
        "DYAPP_EMULATOR_CPUS": str(max(1, CPUS - 1)),
    }
    with (
        worktree_arguments(
            Path(__file__).resolve().parents[2], arguments, temporary
        ) as arguments,
        (temporary / "dyapp-docker.lock").open("a") as lock,
    ):
        fcntl.flock(lock, fcntl.LOCK_EX)
        if "build" in arguments and "run" not in arguments:
            return build_images(arguments, environment, log_file)
        cleanup = None
        if "run" in arguments:
            prefix = arguments[: arguments.index("run")]
            cleanup = ["docker", "compose", *prefix, "stop"]
            # Captured checks must not allocate a pseudo-terminal.
            if environment.get("DYAPP_VERBOSE") != "1":
                index = arguments.index("run") + 1
                arguments.insert(index, "--no-TTY")
        label = "Docker Compose"
        if "--profile" in arguments:
            label += f" {arguments[arguments.index('--profile') + 1]}"
        return run_logged(
            ["docker", "compose", *arguments],
            environment,
            label,
            cleanup,
            log_file,
        )


if __name__ == "__main__":
    try:
        status = main()
    except KeyboardInterrupt:
        status = config_failure(
            "KeyboardInterrupt: interrupted", 130, os.environ
        )
    except Exception:
        status = config_failure(traceback.format_exc(), 1, os.environ)
    sys.exit(status)
