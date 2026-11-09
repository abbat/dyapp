#!/usr/bin/env python3
"""Real Debian 12 desktop benchmark for the test-only raw Tauri/Canvas path."""

import argparse
import datetime
import json
import os
from pathlib import Path
import platform
import re
import selectors
import signal
import socket
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "build"
TICKS = os.sysconf("SC_CLK_TCK")
PAGE = os.sysconf("SC_PAGE_SIZE")
LOG_LIMIT = 65536


def release():
    values = {}
    for line in Path("/etc/os-release").read_text().splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            values[key] = value.strip('"')
    return values


def optional_output(args):
    try:
        return subprocess.run(
            args, capture_output=True, text=True, timeout=5, check=False
        ).stdout.strip()[:4096]
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return "unavailable"


def host_info():
    distro = release()
    if distro.get("ID") != "debian" or distro.get("VERSION_ID") != "12":
        raise RuntimeError("Benchmark requires a real Debian 12 desktop")
    gpu = []
    for device in Path("/sys/class/drm").glob("card*/device"):
        try:
            gpu.append(
                {
                    "card": device.parent.name,
                    "vendor": (device / "vendor").read_text().strip(),
                    "device": (device / "device").read_text().strip(),
                }
            )
        except OSError:
            continue
    return {
        "hostname": socket.gethostname(),
        "uname": platform.uname()._asdict(),
        "source_commit": optional_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"]
        ),
        "os_release": distro,
        "gpu": gpu,
        "webkitgtk": optional_output(
            [
                "dpkg-query",
                "-W",
                "-f=${Package} ${Version}",
                "libwebkit2gtk-4.1-0",
            ]
        ),
        "cpu": optional_output(["lscpu"]),
        "display": {
            key: os.environ.get(key, "")
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "XDG_SESSION_TYPE",
                "GDK_BACKEND",
            )
        },
        "clock_uncertainty": (
            "Rust and JavaScript share host wall clock; Date.now() is "
            "quantized to at least 1 ms. Clock adjustments and scheduling "
            "delay are not isolated."
        ),
    }


def process_tree(root_pid):
    """Return CPU ticks and RSS bytes for app and all surviving descendants."""
    processes = {}
    for path in Path("/proc").glob("[0-9]*"):
        try:
            raw = (path / "stat").read_text()
            fields = raw[raw.rfind(")") + 2 :].split()
            pid = int(path.name)
            processes[pid] = (
                int(fields[1]),
                int(fields[19]),
                int(fields[11]) + int(fields[12]),
                int(fields[21]) * PAGE,
            )
        except (OSError, IndexError, ValueError):
            continue
    tracked = {root_pid}
    for _ in range(12):
        children = {
            pid
            for pid, (parent, _, _, _) in processes.items()
            if parent in tracked
        }
        if children.issubset(tracked):
            break
        tracked.update(children)
    return {
        (pid, processes[pid][1]): (processes[pid][2], processes[pid][3])
        for pid in tracked
        if pid in processes
    }


def sample_process(process, previous, samples, started, now):
    current = process_tree(process.pid)
    ticks = sum(
        max(0, ticks - previous[key][0])
        for key, (ticks, _) in current.items()
        if key in previous
    )
    previous = current
    if current and len(samples) < 200:
        samples.append(
            {
                "seconds": round(now - started, 2),
                "rss_mb": round(
                    sum(rss for _, rss in current.values()) / 1048576, 2
                ),
                "processes": len(current),
            }
        )
    return current, ticks


def drain_output(selector, output_tail):
    for key, _ in selector.select(timeout=0.5):
        chunk = os.read(key.fd, 65536)
        if chunk:
            output_tail = (output_tail + chunk)[-LOG_LIMIT:]
        else:
            selector.unregister(key.fileobj)
    return output_tail


def run_case(app, name, output):
    profile = output / f"{name}-profile"
    profile.mkdir()
    env = dict(os.environ, DYAPP_TEST_CONFIG_DIR=str(profile))
    env.pop("DYAPP_AXE", None)
    process = subprocess.Popen(  # pylint: disable=consider-using-with
        [str(app), "--ui-test", f"--ui-test-step=video-bench:{name}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=env,
        start_new_session=True,
    )
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    started = time.monotonic()
    next_sample = started
    previous = {}
    cpu_ticks = 0
    samples = []
    output_tail = b""
    exit_seen_at = None
    try:
        while process.poll() is None or selector.get_map():
            output_tail = drain_output(selector, output_tail)
            now = time.monotonic()
            if now >= next_sample:
                previous, ticks = sample_process(
                    process, previous, samples, started, now
                )
                cpu_ticks += ticks
                next_sample = now + 0.5
            if process.poll() is not None:
                if exit_seen_at is None:
                    exit_seen_at = time.monotonic()
                if time.monotonic() - exit_seen_at > 2:
                    break  # A WebKit helper may still hold stdout open.
            if time.monotonic() - started > 95:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
                raise RuntimeError(f"{name} exceeded 95 seconds")
        process.wait(timeout=5)
    finally:
        selector.close()
        process.stdout.close()
    text = output_tail.decode("utf-8", errors="replace")
    (output / f"{name}.log").write_text(text)
    metric_pattern = (
        rf"^UI_SMOKE_METRICS:video:{re.escape(name)}:"
        r"(\{[^\n]{1,500}\})$"
    )
    match = re.search(metric_pattern, text, re.M)
    if process.returncode or "UI_SMOKE_PASS:" not in text or not match:
        raise RuntimeError(f"{name} failed; see {output / (name + '.log')}")
    duration = (exit_seen_at or time.monotonic()) - started
    rss = [sample["rss_mb"] for sample in samples]
    return {
        "frame_metrics": json.loads(match[1]),
        "duration_seconds": round(duration, 2),
        "cpu_seconds": round(cpu_ticks / TICKS, 2),
        "cpu_cores": round(cpu_ticks / TICKS / duration, 3),
        "rss_first_mb": rss[0] if rss else None,
        "rss_last_mb": rss[-1] if rss else None,
        "rss_peak_mb": max(rss) if rss else None,
        "samples": samples,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--app", type=Path, default=BUILD / "debug/dyapp-linux"
    )
    parser.add_argument(
        "--host",
        required=True,
        help="human-readable name of the measured desktop",
    )
    args = parser.parse_args()
    valid_name = re.fullmatch(r"[A-Za-z0-9._-]{1,64}", args.host)
    if not valid_name or args.host in (".", ".."):
        parser.error("--host must be a short name without path separators")
    app = args.app.resolve()
    if not app.is_file():
        parser.error(f"debug app not found: {app}")
    info = host_info()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime(
        "%Y%m%dT%H%M%SZ"
    )
    output = BUILD / "video-benchmark" / f"{stamp}-{args.host}"
    output.mkdir(parents=True)
    cases = {}
    for name in ("idle", "360p", "720p"):
        cases[name] = run_case(app, name, output)
    idle = cases["idle"]["cpu_cores"]
    for name in ("360p", "720p"):
        cases[name]["additional_cpu_cores"] = round(
            cases[name]["cpu_cores"] - idle, 3
        )
    (output / "report.json").write_text(
        json.dumps(
            {
                "host": args.host,
                "environment": info,
                "cases": cases,
            },
            indent=2,
        )
        + "\n"
    )
    print(output / "report.json")


if __name__ == "__main__":
    main()
