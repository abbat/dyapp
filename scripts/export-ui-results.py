"""Copy results and tested applications from the latest suite containers."""
import json
from pathlib import Path
import re
import subprocess
import sys

suite, destination = sys.argv[1:]
services = {"linux": ["linux-test"],
            "android": ["android-unit-test", "android-emulator-test"]}[suite]
destination = Path(destination)
destination.mkdir(parents=True, exist_ok=True)
for service in services:
    containers = subprocess.check_output([
        "docker", "ps", "--all", "--quiet",
        "--filter", "label=com.docker.compose.project=dyapp",
        "--filter", f"label=com.docker.compose.service={service}",
    ], text=True).splitlines()
    if not containers:
        sys.exit(f"No {service} container")
    identifier = containers[0]
    container = json.loads(subprocess.check_output(
        ["docker", "inspect", identifier], text=True))[0]
    hostname = container["Config"]["Hostname"]
    if not re.fullmatch(r"[0-9a-f]{12}", hostname):
        sys.exit(f"Unexpected container hostname: {hostname}")
    subprocess.run([
        "docker", "cp", f"{identifier}:/reports/{hostname}",
        str(destination / service),
    ], check=True)
    application = {
        "linux-test": "/app/linux/target/debug/dyapp-linux",
        "android-emulator-test":
            "/app/android/app/build/outputs/apk/debug/app-debug.apk",
    }.get(service)
    if application:
        subprocess.run([
            "docker", "cp", f"{identifier}:{application}",
            str(destination / Path(application).name),
        ], check=True)
