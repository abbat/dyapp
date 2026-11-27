"""Strict shared catalogue sources; no output is written while validating."""

from collections.abc import Mapping
from dataclasses import dataclass
import json
from pathlib import Path
import re
from typing import NoReturn

from translation_templates import ArgumentType, checked_tokens, render

MAX_BYTES = 500 * 1024
KEY = re.compile(r"[a-z][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)+\Z")
TAG = re.compile(r"[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*\Z")
QUALIFIER = re.compile(r"values(?:-[A-Za-z0-9+]+)*\Z")
NATIVE_KEY = re.compile(r"[A-Za-z][A-Za-z0-9_.-]*\Z")
ANDROID_KEY = re.compile(r"[a-z][a-z0-9_]*\Z")


@dataclass(frozen=True)
class Binding:
    platform: str
    key: str
    tokens: dict[str, str]


@dataclass(frozen=True)
class Message:
    arguments: dict[str, ArgumentType]
    bindings: tuple[Binding, ...]
    context: str


@dataclass(frozen=True)
class Catalogue:
    default: str
    languages: dict[str, str]
    messages: dict[str, Message]
    texts: dict[str, dict[str, str]]


def object_map(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    result: dict[str, object] = {}
    for key, item in value.items():
        if not isinstance(key, str):
            raise ValueError(f"{label} keys must be strings")
        result[key] = item
    return result


def string_map(value: object, label: str) -> dict[str, str]:
    result = {}
    for key, item in object_map(value, label).items():
        if not isinstance(item, str):
            raise ValueError(f"{label}.{key} must be a string")
        result[key] = item
    return result


def fields(
    value: Mapping[str, object], expected: set[str], label: str
) -> None:
    if set(value) != expected:
        raise ValueError(f"{label} fields must be: {sorted(expected)}")


def pairs(items: list[tuple[str, object]]) -> dict[str, object]:
    result = {}
    for key, value in items:
        if key in result:
            raise ValueError(f"Duplicate catalogue key: {key}")
        result[key] = value
    return result


def nonfinite(value: str) -> NoReturn:
    raise ValueError(f"Non-finite catalogue number: {value}")


def read(path: Path) -> dict[str, object]:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"Catalogue source must be a regular file: {path}")
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError(f"Catalogue source exceeds {MAX_BYTES} bytes: {path}")
    return object_map(
        json.loads(data, object_pairs_hook=pairs, parse_constant=nonfinite),
        str(path),
    )


def argument_types(value: object) -> dict[str, ArgumentType]:
    result: dict[str, ArgumentType] = {}
    for name, kind in string_map(value, "arguments").items():
        if kind == "string":
            result[name] = "string"
        elif kind == "integer":
            result[name] = "integer"
        else:
            raise ValueError(f"Unknown translation argument type: {kind}")
    return result


def bindings(
    value: object, arguments: dict[str, ArgumentType]
) -> tuple[Binding, ...]:
    result = []
    for platform, entries in object_map(value, "bindings").items():
        for key, tokens in object_map(entries, platform).items():
            pattern = ANDROID_KEY if platform == "android" else NATIVE_KEY
            if pattern.fullmatch(key) is None:
                raise ValueError(f"Invalid {platform} resource key: {key}")
            result.append(
                Binding(
                    platform,
                    key,
                    checked_tokens(
                        platform, arguments, string_map(tokens, key)
                    ),
                )
            )
        if not entries:
            raise ValueError(f"Empty platform binding: {platform}")
    if not result:
        raise ValueError("A message must bind at least one native resource")
    return tuple(result)


def messages(value: object) -> dict[str, Message]:
    result = {}
    used = set()
    for key, raw in object_map(value, "messages").items():
        if KEY.fullmatch(key) is None:
            raise ValueError(f"Invalid canonical message key: {key}")
        entry = object_map(raw, key)
        fields(entry, {"arguments", "bindings", "context"}, key)
        context = entry["context"]
        if not isinstance(context, str) or not context.strip():
            raise ValueError(f"Message context is required: {key}")
        arguments = argument_types(entry["arguments"])
        targets = bindings(entry["bindings"], arguments)
        for target in targets:
            pair = (target.platform, target.key)
            if pair in used:
                raise ValueError(f"Native resource is bound twice: {pair}")
            used.add(pair)
        result[key] = Message(arguments, targets, context)
    if not result:
        raise ValueError("Catalogue messages must not be empty")
    return result


def languages(value: object, default: str) -> dict[str, str]:
    result = string_map(value, "languages")
    if not result or default not in result:
        raise ValueError("Default language must be present")
    for tag, qualifier in result.items():
        if (
            TAG.fullmatch(tag) is None
            or QUALIFIER.fullmatch(qualifier) is None
        ):
            raise ValueError(f"Invalid language/Android qualifier: {tag}")
    if len(set(result.values())) != len(result):
        raise ValueError("Android language qualifiers must be unique")
    if result[default] != "values":
        raise ValueError("Default language must use Android values")
    return result


def load(root: Path) -> Catalogue:
    directory = root / "i18n"
    source = read(directory / "catalogue.json")
    fields(source, {"schema", "default", "languages", "messages"}, "catalogue")
    if type(source["schema"]) is not int or source["schema"] != 1:
        raise ValueError("Unsupported catalogue schema")
    default = source["default"]
    if not isinstance(default, str):
        raise ValueError("Default language must be a string")
    locales = languages(source["languages"], default)
    entries = messages(source["messages"])
    expected = {"catalogue.json"} | {f"{tag}.json" for tag in locales}
    actual = {path.name for path in directory.glob("*.json")}
    if actual != expected:
        raise ValueError("Catalogue language source inventory differs")
    texts = {}
    for tag in locales:
        text = string_map(read(directory / f"{tag}.json"), tag)
        if set(text) != set(entries):
            raise ValueError(f"Message inventory differs in language: {tag}")
        for key, message in entries.items():
            for target in message.bindings:
                render(
                    text[key],
                    message.arguments,
                    target.platform,
                    target.tokens,
                )
        texts[tag] = text
    return Catalogue(default, locales, entries, texts)
