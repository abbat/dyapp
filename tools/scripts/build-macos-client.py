"""Build the Tauri client as a native or universal macOS app."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin")
BINARY = "dyapp-macos"


@dataclass(frozen=True)
class Settings:
    name: str
    identifier: str
    version: str
    minimum: str


def text(value: object) -> str:
    if not isinstance(value, str) or not value:
        raise ValueError("Missing string in macOS application metadata")
    return value


def settings() -> Settings:
    root = ROOT / "clients/macos"
    config = json.loads((root / "tauri.conf.json").read_text())
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    version = text(config["version"])
    if manifest["package"]["version"] != version:
        raise ValueError("macOS Cargo and Tauri versions differ")
    return Settings(
        text(config["productName"]),
        text(config["identifier"]),
        version,
        text(config["bundle"]["macOS"]["minimumSystemVersion"]),
    )


def run(command: list[str], log: Path, minimum: str) -> None:
    environment = dict(os.environ, MACOSX_DEPLOYMENT_TARGET=minimum)
    with log.open("w") as stream:
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=environment,
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=False,
        )
    if result.returncode:
        raise ValueError(
            f"Native command exited {result.returncode}; see {log}"
        )


def camera_description(locale: str) -> str:
    catalogue = ROOT / f"clients/desktop/ui/i18n/{locale}.json"
    return text(
        json.loads(catalogue.read_text())["view.safety.camera.scanning"]
    )


def bundle(binary: Path, destination: Path, config: Settings) -> None:
    if destination.exists() or destination.is_symlink():
        raise ValueError("App output exists; choose a fresh --output path")
    contents = destination / "Contents"
    executable = contents / "MacOS" / BINARY
    executable.parent.mkdir(parents=True)
    resources = contents / "Resources"
    resources.mkdir()
    shutil.copy2(binary, executable)
    shutil.copy2(
        ROOT / "clients/macos/icons/icon.icns", resources / "icon.icns"
    )
    metadata = {
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundlePackageType": "APPL",
        "CFBundleName": config.name,
        "CFBundleDisplayName": config.name,
        "CFBundleIdentifier": config.identifier,
        "CFBundleExecutable": BINARY,
        "CFBundleShortVersionString": config.version,
        "CFBundleVersion": config.version,
        "CFBundleIconFile": "icon.icns",
        "CFBundleDevelopmentRegion": "en",
        "CFBundleLocalizations": ["en", "ru"],
        "DTPlatformName": "macosx",
        "LSMinimumSystemVersion": config.minimum,
        "NSHighResolutionCapable": True,
        "NSCameraUsageDescription": camera_description("en"),
    }
    (contents / "Info.plist").write_bytes(plistlib.dumps(metadata))
    for locale in ("en", "ru"):
        localized = resources / f"{locale}.lproj"
        localized.mkdir()
        description = json.dumps(
            camera_description(locale), ensure_ascii=False
        )
        (localized / "InfoPlist.strings").write_text(
            f'"NSCameraUsageDescription" = {description};\n',
            encoding="utf-8",
        )


def compile_app(
    profile: str, universal: bool, features: str, logs: Path
) -> Path:
    config = settings()
    logs.mkdir(parents=True, exist_ok=True)
    binaries = []
    for target in TARGETS if universal else ("host",):
        command = [
            "cargo",
            "build",
            "--locked",
            "--manifest-path",
            "clients/macos/Cargo.toml",
            "--target-dir",
            str(ROOT / "build"),
        ]
        if profile == "release":
            command.append("--release")
        if features:
            command += ["--features", features]
        if target != "host":
            command += ["--target", target]
        run(command, logs / f"cargo-{target}.log", config.minimum)
        directory = ROOT / "build"
        if target != "host":
            directory /= target
        binaries.append(directory / profile / BINARY)
    if not universal:
        return binaries[0]
    merged = logs / "dyapp-macos-universal"
    run(
        [
            "xcrun",
            "lipo",
            "-create",
            *map(str, binaries),
            "-output",
            str(merged),
        ],
        logs / "lipo.log",
        config.minimum,
    )
    return merged


def build(
    profile: str, universal: bool, features: str, destination: Path, logs: Path
) -> None:
    if platform.system() != "Darwin":
        raise ValueError("The macOS client requires native macOS/Xcode")
    if destination.exists() or destination.is_symlink():
        raise ValueError("App output exists; choose a fresh --output path")
    config = settings()
    binary = compile_app(profile, universal, features, logs)
    actual = set(
        subprocess.check_output(
            ["xcrun", "lipo", "-archs", str(binary)], text=True
        ).split()
    )
    wanted = {"arm64", "x86_64"} if universal else {platform.machine()}
    if actual != wanted:
        raise ValueError(f"macOS architectures {actual}, expected {wanted}")
    bundle(binary, destination, config)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile", choices=("debug", "release"), default="release"
    )
    parser.add_argument("--universal", action="store_true")
    parser.add_argument("--features", default="")
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--log-dir", type=Path, default=ROOT / "build/macos-build"
    )
    parser.add_argument("--print-version", action="store_true")
    args = parser.parse_args()
    if args.print_version:
        print(settings().version)
        return
    destination = (
        args.output or ROOT / f"build/macos/{args.profile}/DYApp.app"
    ).resolve()
    build(
        args.profile,
        args.universal,
        args.features,
        destination,
        args.log_dir.resolve(),
    )
    print(f"OK macos-build {destination}")


if __name__ == "__main__":
    main()
