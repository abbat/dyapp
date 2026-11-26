"""Require a reviewed guard for every scanned untrusted-input entry point."""

from __future__ import annotations

import csv
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
from typing import TypedDict

CLOCK_SPEC = importlib.util.spec_from_file_location(
    "check_clock", Path(__file__).with_name("check-clock.py")
)
assert CLOCK_SPEC is not None and CLOCK_SPEC.loader is not None
CLOCK = importlib.util.module_from_spec(CLOCK_SPEC)
CLOCK_SPEC.loader.exec_module(CLOCK)

ROOT = Path(__file__).resolve().parents[2]
INVENTORY = Path("docs/security/untrusted-inputs.csv")
COMMANDS = "clients/desktop/main.rs"
FIELDS = ["input", "location", "guard_kind", "guard_ref"]
KINDS = {"allow-list", "fuzz", "limit", "test"}
Hit = tuple[str, str]


class FunctionEntry(TypedDict):
    id: str
    kind: str
    start: list[int]
    end: list[int]


ASSOCIATED_DECODE = re.compile(r"::(?:decode|decode_length_delimited)\s*\(")
METHOD_DECODE = re.compile(r"\.(?:decode|identify)\s*\(")
ANY_DECODE = re.compile(
    r"(?:::(?:decode|decode_length_delimited)|\.(?:decode|identify))\s*\("
)
IMAGE_CALL = re.compile(
    r"\b(?:image::(?:load_from_memory(?:_with_format)?|open)"
    r"|(?:image::)?ImageReader::(?:new|with_format)"
    r"|(?:image_webp::)?WebPDecoder::new"
    r"|(?:jpeg_decoder::|zune_jpeg::)?JpegDecoder::new)\s*\("
)
ARCHIVE_CALL = re.compile(
    r"\b(?:sticker_tar::read|(?:tar::)?Archive::new)\s*\("
)
EXPORT = re.compile(r"#\s*\[\s*uniffi::export(?:\s*\([^]]*\))?\s*\]")
FFI_INPUT = re.compile(
    r"\b(?:String|str)\b|\bVec\s*<\s*u8\s*>|&\s*\[\s*u8\s*\]"
)
DECLARATION = r"\b(?:const|fn|struct|enum|trait|type)\s+{}\b"
CONSTANT = r"\bconst\s+{}\b"
TEST_ATTRIBUTE = re.compile(r"#\s*\[\s*(?:\w+::)*test(?:\s*\([^]]*\))?\s*\]")


def scannable(name: str) -> bool:
    path = PurePosixPath(name)
    return not name.startswith("rust/crates/") and (
        name.startswith("rust/") or path.parts[:2] == ("clients", "desktop")
    )


def entry_name(entry: FunctionEntry) -> str:
    names = re.findall(r"(?:^|::)fn\s+(\w+)", entry["id"])
    if not names:
        raise ValueError(f"Function entry has no name: {entry['id']}")
    return str(names[-1])


def test_entry(entry: FunctionEntry) -> bool:
    return "#[cfg(test)]" in "".join(entry["id"].split())


def entry_lines(text: str, entry: FunctionEntry) -> tuple[int, int, str]:
    lines = text.splitlines(keepends=True)
    start, end = entry["start"][0], entry["end"][0]
    if start < 1 or end < start or end > len(lines):
        raise ValueError(f"Invalid function span: {entry['id']}")
    return start, end, "".join(lines[start - 1 : end])


def parameters(text: str, name: str) -> str:
    matches = list(re.finditer(rf"\bfn\s+{re.escape(name)}\s*\(", text))
    if not matches:
        raise ValueError(f"Missing signature of {name}")
    start = matches[-1].end()
    depth = 1
    for index in range(start, len(text)):
        if text[index] == "(":
            depth += 1
        elif text[index] == ")":
            depth -= 1
            if depth == 0:
                return text[start:index]
    raise ValueError(f"Unterminated signature of {name}")


def function_hits(path: str, text: str, entry: FunctionEntry) -> set[Hit]:
    _start, _end, source = entry_lines(text, entry)
    name = entry_name(entry)
    location = f"{path}#{name}"
    result: set[Hit] = set()
    if ASSOCIATED_DECODE.search(source):
        result.add(("prost", location))
    if IMAGE_CALL.search(source):
        result.add(("image", location))
    if ARCHIVE_CALL.search(source):
        result.add(("archive", location))
    if found := METHOD_DECODE.search(source):
        prefix = source[: found.start()]
        if "quircs" in source or ".identify(" in source:
            result.add(("qr", location))
        elif re.search(
            r"ImageReader|WebPDecoder|JpegDecoder|Decoder::new", prefix
        ):
            result.add(("image", location))
        else:
            result.add(("decode", location))
    if "request_response::CodecforProtoCodec" in "".join(
        entry["id"].split()
    ) and name in {"read_request", "read_response"}:
        result.add(("p2p-codec", location))
    if path.startswith("rust/p2p-net/") and re.search(
        r"\b(?:kad|gossipsub)::(?:Event|Message)\b", source
    ):
        result.add(("p2p-event", location))
    if EXPORT.search(source) and FFI_INPUT.search(parameters(source, name)):
        result.add(("uniffi", location))
    return result


def command_hits(text: str) -> set[Hit]:
    match = re.search(r"tauri::generate_handler!\s*\[(.*?)\]\s*;", text, re.S)
    if not match:
        raise ValueError(f"{COMMANDS}: missing generate_handler inventory")
    result = set()
    lines = [
        raw.strip()
        for raw in match.group(1).splitlines()
        if raw.strip()
        and not raw.strip().startswith("#[")
        and not raw.strip().startswith("//")
    ]
    for index, line in enumerate(lines):
        if not line.endswith(",") and index + 1 != len(lines):
            raise ValueError(f"{COMMANDS}: malformed command line: {line}")
        command = line.removesuffix(",").strip()
        if not re.fullmatch(r"\w+(?:::\w+)*", command):
            raise ValueError(f"{COMMANDS}: malformed command: {command}")
        result.add(("tauri", f"{COMMANDS}#{command}"))
    if not result:
        raise ValueError(f"{COMMANDS}: empty generate_handler inventory")
    return result


def external_test(path: Path, tests: set[Path]) -> bool:
    return (
        "tests" in path.parts
        or path.name in {"test.rs", "tests.rs"}
        or path.name.endswith("_tests.rs")
        or any(
            item in tests
            for item in (path, *path.parents, path.with_suffix(""))
        )
    )


def scan_sources(
    sources: dict[str, str],
    functions: dict[str, list[FunctionEntry]],
) -> set[Hit]:
    by_path = {Path(name): name for name in sources}
    tests = CLOCK.test_modules(
        {Path(name): text for name, text in sources.items()}
    )
    result: set[Hit] = set()
    for path, name in by_path.items():
        if not scannable(name) or external_test(path, tests):
            continue
        text = sources[name]
        entries = functions.get(name, [])
        for entry in entries:
            if entry["kind"] == "function" and not test_entry(entry):
                result |= function_hits(name, text, entry)
        for call in ANY_DECODE.finditer(text):
            line = text.count("\n", 0, call.start()) + 1
            owners = []
            for entry in entries:
                if entry["kind"] != "function":
                    continue
                start, end, _source = entry_lines(text, entry)
                if start <= line <= end:
                    owners.append(entry)
            if not owners:
                raise ValueError(
                    f"{name}:{line}: decoder outside a named function"
                )
    result |= command_hits(sources[COMMANDS])
    return result


def read_sources(root: Path) -> dict[str, str]:
    raw = subprocess.check_output(
        ["git", "ls-files", "-z", "--", "*.rs"], cwd=root
    )
    names = sorted(
        name
        for name in (os.fsdecode(item) for item in raw.split(b"\0") if item)
        if not name.startswith("rust/crates/")
    )
    return {name: (root / name).read_text(encoding="utf-8") for name in names}


def parse_functions(
    root: Path, sources: dict[str, str]
) -> dict[str, list[FunctionEntry]]:
    command = [
        "cargo",
        "run",
        "--quiet",
        "--locked",
        "--manifest-path",
        "tools/rust-functions/Cargo.toml",
        "--",
        *sources,
    ]
    run = subprocess.run(command, cwd=root, capture_output=True, text=True)
    if run.returncode:
        raise ValueError(f"rust-functions failed: {run.stderr[-4096:]}")
    result = {}
    for line in run.stdout.splitlines():
        record = json.loads(line)
        name = record["path"]
        if name in result or name not in sources:
            raise ValueError(f"Unexpected or duplicate parser record: {name}")
        result[name] = record["functions"]
    if result.keys() != sources.keys():
        raise ValueError("Incomplete rust-functions output")
    return result


def read_inventory(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream)
        if reader.fieldnames != FIELDS:
            raise ValueError(f"{path}: expected columns {','.join(FIELDS)}")
        rows = []
        for row in reader:
            if any(row.get(field) is None for field in FIELDS):
                raise ValueError(f"{path}: incomplete inventory row")
            rows.append({field: row[field].strip() for field in FIELDS})
        return rows


def guard_errors(
    row: dict[str, str],
    sources: dict[str, str],
    functions: dict[str, list[FunctionEntry]],
    tests: set[Path],
) -> list[str]:
    kind = row["guard_kind"]
    if kind not in KINDS:
        return [f"{row['location']}: unknown guard kind {kind}"]
    refs = [
        item.strip() for item in row["guard_ref"].split(";") if item.strip()
    ]
    if not refs:
        return [f"{row['location']}: empty guard reference"]
    result = []
    for ref in refs:
        path, separator, symbol = ref.partition("#")
        text = sources.get(path)
        if text is None:
            result.append(f"{row['location']}: missing guard source {path}")
        elif kind == "fuzz" and (separator or "fuzz_target!" not in text):
            result.append(f"{row['location']}: invalid fuzz target {ref}")
        elif kind == "test" and (
            not separator
            or not any(
                (test_entry(entry) or external_test(Path(path), tests))
                and entry_name(entry) == symbol
                and TEST_ATTRIBUTE.search(entry_lines(text, entry)[2])
                for entry in functions.get(path, [])
            )
        ):
            result.append(f"{row['location']}: missing test guard {ref}")
        elif kind == "limit" and (
            not separator
            or not re.search(CONSTANT.format(re.escape(symbol)), text)
        ):
            result.append(f"{row['location']}: missing limit guard {ref}")
        elif kind == "allow-list" and (
            not separator
            or not re.search(DECLARATION.format(re.escape(symbol)), text)
        ):
            result.append(f"{row['location']}: missing allow-list guard {ref}")
    return result


def inventory_errors(
    rows: list[dict[str, str]],
    hits: set[Hit],
    sources: dict[str, str],
    functions: dict[str, list[FunctionEntry]],
) -> list[str]:
    result = []
    seen = set()
    tests = CLOCK.test_modules(
        {Path(name): text for name, text in sources.items()}
    )
    for row in rows:
        key = row["input"], row["location"]
        if key in seen:
            result.append(f"duplicate inventory row: {key[0]} {key[1]}")
        seen.add(key)
        if key not in hits:
            result.append(f"stale inventory row: {key[0]} {key[1]}")
        result += guard_errors(row, sources, functions, tests)
    for kind, location in sorted(hits - seen):
        result.append(f"missing inventory row: {kind} {location}")
    if not rows:
        result.append("untrusted-input inventory is empty")
    return sorted(result)


def run(root: Path) -> list[str]:
    sources = read_sources(root)
    functions = parse_functions(root, sources)
    hits = scan_sources(sources, functions)
    rows = read_inventory(root / INVENTORY)
    return inventory_errors(rows, hits, sources, functions)


def main() -> int:
    try:
        errors = run(ROOT)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"untrusted-input check failed: {error}", file=sys.stderr)
        return 1
    print("\n".join(errors) or "untrusted-input inventory is current")
    return bool(errors)


if __name__ == "__main__":
    sys.exit(main())
