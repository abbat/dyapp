"""Read bounded native visual archives without trusting archive paths."""

import argparse
from itertools import product
import json
import math
from pathlib import Path
import re
import struct
import tarfile

SCREENS = (
    "owner-code",
    "owner-malformed",
    "owner-expired",
    "owner-preparing",
    "owner-prepare-error",
    "owner-confirm",
    "owner-confirm-expired",
    "owner-sending",
    "owner-send-error",
    "owner-send-expired",
    "owner-send-check-error",
    "owner-sent",
)
RECEIVE_SCREENS = (
    "link-receive-intro",
    "link-receive-wizard-intro",
    "link-receive-starting",
    "link-receive-waiting",
    "link-receive-receiving",
    "link-receive-installing",
    "link-receive-cancelled",
    "link-receive-expired",
    "link-receive-invalidated",
    "link-receive-failed-admission",
    "link-receive-failed-reservation",
    "link-receive-failed-destination",
    "link-receive-failed-unconfirmed",
    "link-receive-received",
    "link-receive-durability",
    "link-setup-running",
    "link-setup-complete",
    "link-setup-review",
    "link-setup-private-review",
    "link-setup-authorization",
    "link-setup-revoked",
    "link-setup-unlisted",
    "link-setup-error",
    "link-receive-start-error",
    "link-receive-poll-error",
)
VIEW_SCREENS = (
    "settings-views",
    "settings-views-loading",
    "settings-views-empty",
    "settings-views-error",
    "settings-views-stale",
)
UPDATE_SCREENS = (
    "update-notice-package",
    "update-notice-source",
    "update-notice-saving",
    "update-notice-write-error",
    "update-notice-open-error",
    "update-notice-permission-allowed",
    "update-notice-permission-blocked",
    "update-notice-permission-pending",
    "update-notice-permission-failed",
    "update-notice-permission-unknown",
)
ACTIVITY_SCREENS = (
    "activity-list-active",
    "activity-list-unknown",
    "activity-chat-active",
    "activity-chat-unknown",
    "activity-matches",
    "activity-nobody",
    "activity-loading",
    "activity-error",
)
RECOVERY_SCREENS = (
    "search-recovery-review",
    "search-recovery-busy",
    "search-recovery-pending",
    "search-recovery-sent",
    "search-recovery-read-error",
    "search-recovery-confirm-error",
    "search-recovery-retry-error",
)
MAX_BYTES = 512 * 1024 * 1024


def allowed_size(
    member: tarfile.TarInfo, folders: set[str], pattern: re.Pattern[str]
) -> int:
    name = member.name.rstrip("/")
    if member.isdir() and name in folders:
        return 0
    if not member.isfile() or pattern.fullmatch(name) is None:
        raise ValueError("Unexpected native archive entry")
    limit = 1024 * 1024 if name.endswith(".json") else 16 * 1024 * 1024
    if not 0 < member.size <= limit:
        raise ValueError("Native archive entry exceeds its limit")
    return member.size


def unpack(
    archive: Path, output: Path, width: int, kind: str = "visual"
) -> None:
    if archive.stat().st_size > MAX_BYTES:
        raise ValueError("Native visual archive exceeds its limit")
    if kind not in (
        "visual",
        "ime",
        "receive",
        "views",
        "updates",
        "activity",
        "recovery",
    ):
        raise ValueError("Unknown native evidence kind")
    folder = {
        "visual": "owner-approval-visual",
        "ime": "owner-approval-ime",
        "receive": "receive-visual",
        "views": "received-views-visual",
        "updates": "update-notice-visual",
        "activity": "activity-visual",
        "recovery": "search-recovery-visual",
    }[kind]
    prefix = f"files/{folder}/{width}"
    folders = {"files", f"files/{folder}", prefix}
    screen = {
        "receive": r"link-[a-z-]+",
        "views": r"settings-views(?:-[a-z]+)?",
        "updates": r"update-notice-[a-z-]+",
        "activity": r"activity-[a-z-]+",
        "recovery": r"search-recovery-[a-z-]+",
    }.get(kind, r"owner-[a-z-]+")
    suffix = r"[0-9]{1,4}" if kind == "ime" else r"(top|bottom)"
    pattern = re.compile(
        re.escape(prefix)
        + r"/(manifest\.json|"
        + screen
        + r"-(en|ru)-"
        + str(width)
        + r"-(light|dark)-(100|200)-"
        + suffix
        + r"\.png)"
    )
    with tarfile.open(archive, "r:") as source:
        members = source.getmembers()
        names = [member.name.rstrip("/") for member in members]
        maximum = 512 if kind == "receive" else 256
        if len(members) > maximum or len(names) != len(set(names)):
            raise ValueError("Duplicate or excessive archive members")
        total = sum(
            allowed_size(member, folders, pattern) for member in members
        )
        if total > MAX_BYTES:
            raise ValueError("Native archive exceeds its content limit")
        output.mkdir(parents=True, exist_ok=False)
        for member in members:
            if member.isfile():
                stream = source.extractfile(member)
                if stream is None:
                    raise ValueError("Missing native archive content")
                with (
                    stream,
                    (output / Path(member.name).name).open("xb") as target,
                ):
                    target.write(stream.read())


def validate(
    output: Path, width: int, source: str, screens: tuple[str, ...] = SCREENS
) -> int:
    manifest = json.loads((output / "manifest.json").read_text())
    if (
        manifest.get("source_sha") != source
        or manifest.get("requested_width_dp") != width
        or manifest.get("complete") is not True
    ):
        raise ValueError("Incomplete or mismatched native visual manifest")
    expected = {
        f"{screen}-{language}-{width}-{theme}-{font}-{edge}.png"
        for screen, language, theme, font, edge in product(
            screens,
            ("en", "ru"),
            ("light", "dark"),
            (100, 200),
            ("top", "bottom"),
        )
    }
    frames = manifest.get("frames")
    if not isinstance(frames, list):
        raise ValueError("Missing native frame inventory")
    actual = set()
    for frame in frames:
        name = frame.get("file")
        if not isinstance(name, str) or name not in expected or name in actual:
            raise ValueError("Unexpected or duplicate native frame")
        actual.add(name)
        native_scale(frame, name)
        data = (output / name).read_bytes()
        if data[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR":
            raise ValueError("Invalid native PNG header")
        dimensions = struct.unpack(">II", data[16:24])
        if dimensions != (frame.get("pixel_width"), frame.get("pixel_height")):
            raise ValueError("Native PNG dimensions disagree with manifest")
        if min(dimensions) <= 0 or frame.get("viewport_width_dp") != width:
            raise ValueError("Native viewport mismatch")
    if actual != expected or {p.name for p in output.iterdir()} != expected | {
        "manifest.json"
    }:
        raise ValueError("Incomplete native visual matrix")
    return len(actual)


def native_scale(frame: dict[str, object], name: str) -> None:
    expected = int(name.rsplit("-", 2)[1])
    for key in ("actual_font_scale", "actual_density"):
        value = frame.get(key)
        if (
            isinstance(value, bool)
            or not isinstance(value, (int, float))
            or not math.isfinite(value)
            or value <= 0
        ):
            raise ValueError("Missing or invalid native text density witness")
    if frame.get("font_percent") != expected:
        raise ValueError("Requested native font scale disagrees with frame")
    if frame["actual_font_scale"] != expected / 100:
        raise ValueError(
            "Actual native font scale disagrees with requested scale"
        )


def rectangle(value: object) -> list[float]:
    if not isinstance(value, list) or len(value) != 4:
        raise ValueError("Missing native screen rectangle")
    result = []
    for item in value:
        if (
            isinstance(item, bool)
            or not isinstance(item, (int, float))
            or not math.isfinite(item)
        ):
            raise ValueError("Invalid native screen coordinate")
        result.append(float(item))
    if result[0] >= result[2] or result[1] >= result[3]:
        raise ValueError("Empty native screen rectangle")
    return result


def ime_geometry(frame: dict[str, object]) -> None:
    visible = rectangle(frame.get("visible_screen_px"))
    for key in ("glyph_screen_px", "cancel_screen_px", "next_screen_px"):
        bounds = rectangle(frame.get(key))
        if not (
            visible[0] <= bounds[0] < bounds[2] <= visible[2]
            and visible[1] <= bounds[1] < bounds[3] <= visible[3]
        ):
            raise ValueError("Native IME hides a glyph or action")


def ime_frame(output: Path, frame: dict[str, object], name: str) -> None:
    if frame.get("selection") != int(name.rsplit("-", 1)[1][:-4]):
        raise ValueError("Real-IME selection mismatch")
    ime_geometry(frame)
    data = (output / name).read_bytes()
    if data[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR":
        raise ValueError("Invalid real-IME PNG header")
    if min(struct.unpack(">II", data[16:24])) <= 0:
        raise ValueError("Invalid real-IME PNG dimensions")


def validate_ime(output: Path, width: int, source: str) -> int:
    manifest = json.loads((output / "manifest.json").read_text())
    length = manifest.get("code_length")
    if (
        manifest.get("source_sha") != source
        or manifest.get("width_dp") != width
        or manifest.get("complete") is not True
        or not isinstance(length, int)
        or not 2 <= length <= 4096
    ):
        raise ValueError("Incomplete or mismatched real-IME manifest")
    expected = {
        f"owner-ime-{language}-{width}-{theme}-{font}-{index}.png"
        for language, theme, font, index in product(
            ("en", "ru"),
            ("light", "dark"),
            (100, 200),
            (0, length // 2, length),
        )
    }
    frames = manifest.get("frames")
    if not isinstance(frames, list):
        raise ValueError("Missing real-IME frame inventory")
    actual = set()
    for frame in frames:
        if not isinstance(frame, dict):
            raise ValueError("Invalid real-IME frame")
        name = frame.get("file")
        if not isinstance(name, str) or name not in expected or name in actual:
            raise ValueError("Unexpected or duplicate real-IME frame")
        actual.add(name)
        native_scale(frame, name)
        ime_frame(output, frame, name)
    if actual != expected or {p.name for p in output.iterdir()} != expected | {
        "manifest.json"
    }:
        raise ValueError("Incomplete real-IME matrix")
    return len(actual)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--width", type=int, choices=(360, 412), required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument(
        "--kind",
        choices=(
            "visual",
            "ime",
            "receive",
            "views",
            "updates",
            "activity",
            "recovery",
        ),
        default="visual",
    )
    args = parser.parse_args()
    if re.fullmatch(r"[0-9a-f]{40}", args.source) is None:
        parser.error("Source must be an exact commit SHA")
    unpack(args.archive, args.output, args.width, args.kind)
    if args.kind == "ime":
        count = validate_ime(args.output, args.width, args.source)
    else:
        screens = {
            "receive": RECEIVE_SCREENS,
            "views": VIEW_SCREENS,
            "updates": UPDATE_SCREENS,
            "activity": ACTIVITY_SCREENS,
            "recovery": RECOVERY_SCREENS,
        }.get(args.kind, SCREENS)
        count = validate(args.output, args.width, args.source, screens)
    print(f"OK native visual matrix: {count} frames")


if __name__ == "__main__":
    main()
