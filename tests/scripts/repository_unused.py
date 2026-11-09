"""Find script files without a repository entry point."""
import ast
from pathlib import PurePosixPath

# Exact manual entry points may be listed with a reason; stale entries fail.
SCRIPT_ENTRIES: dict[str, str] = {}


def imported_modules(source: str) -> set[str]:
    result: set[str] = set()
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Import):
            result.update(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            prefix = node.module + "." if node.module else ""
            result.update(prefix + alias.name for alias in node.names)
            if node.module:
                result.add(node.module)
    return result


def references(source: str, script: str, modules: set[str]) -> bool:
    path = PurePosixPath(script)
    if script in source or path.name in source:
        return True
    if path.suffix != ".py":
        return False
    return any(module.split(".")[-1] == path.stem for module in modules)


def is_script(name: str) -> bool:
    if not name.startswith(("tests/scripts/", "tools/")):
        return False
    return PurePosixPath(name).suffix in {".py", ".sh", ".mjs"}


def reachable(graph: dict[str, set[str]], roots: set[str]) -> set[str]:
    found: set[str] = set()
    pending = list(roots)
    while pending:
        name = pending.pop()
        if name in found:
            continue
        found.add(name)
        pending.extend(graph.get(name, set()) - found)
    return found


def script_reference_errors(
    texts: dict[str, str], entries: dict[str, str] | None = None,
) -> list[str]:
    entries = SCRIPT_ENTRIES if entries is None else entries
    scripts = {name for name in texts if is_script(name)}
    graph: dict[str, set[str]] = {}
    roots: set[str] = set()
    for name, source in texts.items():
        modules = imported_modules(source) if name.endswith(".py") else set()
        targets = {script for script in scripts if script != name
                   if references(source, script, modules)}
        if name in scripts:
            graph[name] = targets
        else:
            roots.update(targets)
    ordinary = reachable(graph, roots)
    errors = []
    for name, reason in sorted(entries.items()):
        if not isinstance(reason, str) or not reason.strip():
            errors.append(f"{name}: script entry needs a reason")
        elif name not in scripts or name in ordinary:
            errors.append(f"{name}: stale script entry")
    found = reachable(graph, roots | (scripts & entries.keys()))
    errors.extend(f"{name}: unreferenced script"
                  for name in sorted(scripts - found))
    return errors
