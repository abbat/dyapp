"""Typed named templates and checked native placeholder projection."""

from collections.abc import Mapping
import re
from string import Formatter
from typing import Literal

ArgumentType = Literal["string", "integer"]
Arguments = Mapping[str, ArgumentType]
NAME = re.compile(r"[A-Za-z][A-Za-z0-9_]*\Z")
TOKENS = {
    "android": re.compile(r"%([1-9][0-9]*)\$([sd])\Z"),
    "apple": re.compile(r"%([1-9][0-9]*)\$(@|d|lld)\Z"),
    "desktop": re.compile(r"\{([A-Za-z][A-Za-z0-9_]*)\}\Z"),
}


def parts(text: str, arguments: Arguments) -> list[tuple[str, str | None]]:
    """Reject implicit fields, expressions and lost arguments."""
    if not text.strip() or "\0" in text:
        raise ValueError("Translation must contain nonempty, non-NUL text")
    for name, kind in arguments.items():
        if NAME.fullmatch(name) is None or kind not in ("string", "integer"):
            raise ValueError(f"Invalid typed translation argument: {name}")
    result: list[tuple[str, str | None]] = []
    used = set()
    for literal, field, spec, conversion in Formatter().parse(text):
        if spec or conversion:
            raise ValueError(
                "Translation fields cannot use format expressions"
            )
        if field is not None:
            if field not in arguments or NAME.fullmatch(field) is None:
                raise ValueError(f"Unknown translation argument: {field}")
            used.add(field)
        if result and result[-1][1] is None:
            literal = result.pop()[0] + literal
        result.append((literal, field))
    if used != set(arguments):
        raise ValueError("Translation does not use every declared argument")
    return result


def checked_tokens(
    platform: str, arguments: Arguments, tokens: Mapping[str, str]
) -> dict[str, str]:
    """A native binding must retain every argument and its concrete type."""
    if platform not in TOKENS:
        raise ValueError(f"Unknown translation platform: {platform}")
    if set(tokens) != set(arguments):
        raise ValueError("Native binding arguments differ from the template")
    slots = []
    for name, token in tokens.items():
        match = TOKENS[platform].fullmatch(token)
        if match is None:
            raise ValueError(f"Invalid {platform} translation token: {name}")
        slots.append(match[1])
        if platform != "desktop":
            kind = "string" if match[2] in ("s", "@") else "integer"
            if kind != arguments[name]:
                raise ValueError(f"Native translation argument type: {name}")
    if len(set(slots)) != len(slots):
        raise ValueError("Native arguments cannot share a position or name")
    if platform != "desktop" and sorted(map(int, slots)) != list(
        range(1, len(slots) + 1)
    ):
        raise ValueError("Native argument positions must be contiguous")
    return dict(tokens)


def render(
    text: str,
    arguments: Arguments,
    platform: str,
    tokens: Mapping[str, str],
) -> str:
    """Project a template without evaluating values or expressions."""
    chunks = parts(text, arguments)
    native = checked_tokens(platform, arguments, tokens)
    result = []
    names = {token[1:-1] for token in native.values()}
    for literal, field in chunks:
        if platform == "desktop":
            literal_fields = set(
                re.findall(r"\{([A-Za-z][A-Za-z0-9_]*)\}", literal)
            )
            if literal_fields & names:
                raise ValueError(
                    "Literal braces collide with a native argument"
                )
        elif native:
            literal = literal.replace("%", "%%")
        result.append(literal)
        if field is not None:
            result.append(native[field])
    return "".join(result)
