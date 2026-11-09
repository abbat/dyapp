"""Finite desktop translation domains from shared native contracts."""

import re

from repository_translations import code, literals, scale_options

UI = "clients/desktop/ui/"
TYPE_DOMAINS = (
    ("identity.mode.", "identity-state.js", "StorageMode"),
    ("network.row.", "ipc.js", "NetworkCheckRow"),
    ("node.", "ipc.js", "NodeStatus"),
    ("search.failure.", "ipc.js", "SearchFailure"),
    ("chats.state.", "chat-state.js", "ConversationState"),
    ("chats.delivery.", "chat-state.js", "Delivery"),
    ("view.safety.scan.", "ipc.js", "QrScan"),
    ("chats.blocked.", "ipc.js", "BlockResult"),
    ("search.blocked.", "ipc.js", "BlockResult"),
    ("settings.blocked.", "ipc.js", "BlockResult"),
    ("profile.problem.", "profile-fields.js", "ProfileView"),
)


def typedef_values(source: str, name: str) -> list[str]:
    declarations = re.finditer(r"@typedef\s*\{([\s\S]*?)\}\s+(\w+)", source)
    matches = [match for match in declarations if match[2] == name]
    if len(matches) != 1:
        raise ValueError(f"Missing or duplicate native UI type: {name}")
    values = re.findall(r"'([^']+)'", matches[0][1])
    if not values:
        raise ValueError(f"Type has no finite string domain: {name}")
    return sorted(set(values))


def function_bodies(source: str, name: str) -> list[str]:
    source = code(source)
    masked = re.sub(
        r'"(?:\\.|[^"\\])*"', lambda match: " " * len(match[0]), source
    )
    result = []
    for match in re.finditer(r"\bfn\s+" + re.escape(name) + r"\b", masked):
        start = masked.index("{", match.end())
        depth = 0
        for token in re.finditer(r"[{}]", masked[start:]):
            depth += 1 if token[0] == "{" else -1
            if depth == 0:
                result.append(source[start : start + token.end()])
                break
        else:
            raise ValueError(f"Unclosed native function: {name}")
    if not result:
        raise ValueError(f"Missing native function: {name}")
    return result


def function_values(source: str, name: str) -> set[str]:
    return set().union(
        *(literals(body) for body in function_bodies(source, name))
    )


def documented_errors(source: str, name: str) -> list[str]:
    pattern = (
        r"((?:^///[^\n]*\n)+)(?:#\[[^\]]*\]\s*)*"
        r"(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+"
        r"NAME\b"
    ).replace("NAME", re.escape(name))
    matches = list(re.finditer(pattern, source, re.M))
    if len(matches) != 1:
        raise ValueError(f"Missing or ambiguous error contract: {name}")
    docs = re.sub(r"^/// ?", "", matches[0][1], flags=re.M)
    section = re.search(r"Errors are([\s\S]*)", docs)
    if section is None:
        raise ValueError(f"No explicit error contract: {name}")
    values = re.findall(
        chr(96) + r"([A-Za-z][A-Za-z0-9]*)" + chr(96), section[1]
    )
    if not values:
        raise ValueError(f"Empty error contract: {name}")
    return sorted(set(values))


def native_domains(texts: dict[str, str]) -> dict[str, list[str]]:
    domains = {
        prefix: [
            prefix + value
            for value in typedef_values(texts[UI + filename], name)
        ]
        for prefix, filename, name in TYPE_DOMAINS
    }
    options = scale_options(texts["rust/profile/src/scales.rs"])
    domains["profile.field."] = ["profile.field." + key for key in options]
    domains["profile.value."] = [
        f"profile.value.{key}.{value}"
        for key, values in options.items()
        for value in values
    ]
    domains["profile.value.gender."] = [
        "profile.value.gender." + value for value in options["gender"]
    ]
    for prefix, path, name in (
        ("view.like.", "clients/desktop/chats.rs", "like"),
        ("view.safety.confirm.", "clients/desktop/safety.rs", "confirm"),
    ):
        domains[prefix] = [
            prefix + value for value in documented_errors(texts[path], name)
        ]
    domains["recovery."] = [
        "recovery." + value
        for value in sorted(
            function_values(texts["clients/desktop/slots.rs"], "open_slot")
        )
    ]
    profile = {
        "profile.problem." + value
        for value in function_values(
            texts["clients/desktop/profile.rs"], "code"
        )
    }
    if profile != set(domains["profile.problem."]):
        raise ValueError("ProfileView problem codes differ from native codes")
    return domains


def direct_native_keys(texts: dict[str, str]) -> list[str]:
    keys = function_values(texts["clients/desktop/system_store.rs"], "key")
    result = sorted(key for key in keys if key.startswith("storage.system."))
    if not result:
        raise ValueError("Missing native storage translation keys")
    return result
