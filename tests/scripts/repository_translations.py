"""Check Apple catalogue references and finite native metadata."""

import json
import re

APPLE = "clients/ios/DYApp/Localizable.xcstrings"
SCALES = "rust/profile/src/scales.rs"
PUBLISH = "rust/ffi/src/profile_publish.rs"
TOKENS = re.compile(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*', re.S)
SCALE = re.compile(
    r"scale!\(\s*(\w+),\s*\d+,\s*(?:true|false),\s*"
    r"\[([^]]*)]\s*,?\s*\)",
    re.S,
)
VARIANT = re.compile(r"(\w+)\s*=\s*\d[\d_]*")
ENTRY = re.compile(r'entry!\(\s*(\w+),\s*"([^"]+)"\s*\)')


def block_end(source: str, start: int) -> int:
    depth = 1
    for marker in re.finditer(r"/\*|\*/", source[start + 2 :]):
        depth += 1 if marker[0] == "/*" else -1
        if depth == 0:
            return start + 2 + marker.end()
    raise ValueError("Unterminated source block comment")


def code(source: str) -> str:
    parts = []
    cursor = 0
    while match := TOKENS.search(source, cursor):
        parts.append(source[cursor : match.start()])
        if match[0].startswith('"'):
            parts.append(match[0])
            cursor = match.end()
        elif match[0].startswith("/*"):
            cursor = block_end(source, match.start())
        else:
            cursor = match.end()
    parts.append(source[cursor:])
    return "".join(parts)


def literals(source: str) -> set[str]:
    return {
        match[0][1:-1]
        for match in TOKENS.finditer(code(source))
        if match[0].startswith('"')
    }


def scale_options(source: str) -> dict[str, list[str]]:
    definitions = {}
    for name, body in SCALE.findall(code(source)):
        variants = VARIANT.findall(body)
        if not variants or VARIANT.sub("", body).strip(" \t\r\n,"):
            raise ValueError(f"Unsupported profile scale definition: {name}")
        if name in definitions:
            raise ValueError(f"Duplicate profile scale definition: {name}")
        definitions[name] = variants
    registry = re.search(
        r"pub const SCALES:\s*&\[Scale]\s*=\s*&\[(.*?)];", code(source), re.S
    )
    if registry is None:
        raise ValueError("Missing shared profile scale registry")
    entries = ENTRY.findall(registry[1])
    if not entries or ENTRY.sub("", registry[1]).strip(" \t\r\n,"):
        raise ValueError("Unsupported shared profile scale registry")
    result = {}
    for name, key in entries:
        if name not in definitions or key in result:
            raise ValueError(f"Invalid shared profile scale entry: {key}")
        result[key] = definitions[name]
    return result


def dynamic_apple_keys(texts: dict[str, str], sources: str) -> set[str]:
    result: set[str] = set()
    fields = re.search(r'"profile\.field\."\s*\+', sources)
    values = re.search(r'"profile\.value\."\s*\+', sources)
    if fields or values:
        for field, options in scale_options(texts[SCALES]).items():
            if fields:
                result.add("profile.field." + field)
            if values:
                result.update(
                    f"profile.value.{field}.{option}" for option in options
                )
    if re.search(r'"profile\.publishBlocked\."\s*\+', sources):
        reasons = re.findall(
            r'Block::\w+\s*=>\s*"([^"]+)"', code(texts[PUBLISH])
        )
        if not reasons:
            raise ValueError("Missing native publication block codes")
        result.update("profile.publishBlocked." + reason for reason in reasons)
    return result


def unused_apple_errors(texts: dict[str, str]) -> list[str]:
    catalogue = json.loads(texts[APPLE])["strings"]
    sources = [
        source
        for name, source in texts.items()
        if name.startswith(("clients/ios/DYApp/", "clients/macos/src/"))
        if name.endswith(".swift")
    ]
    used = set().union(*(literals(source) for source in sources))
    used.update(dynamic_apple_keys(texts, "\n".join(map(code, sources))))
    return [
        f"{APPLE}: unused translation key {key}"
        for key in sorted(catalogue.keys() - used)
    ]
