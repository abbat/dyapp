"""Run the real JavaScript/HTML translation checker with native metadata."""
import json
from pathlib import Path
import subprocess

from desktop_translation_domains import (
    TYPE_DOMAINS, UI, direct_native_keys, native_domains,
)

ROOT = Path(__file__).resolve().parents[2]
CATALOGUES = (UI + "i18n/en.json", UI + "i18n/ru.json")
NATIVE = (
    "rust/profile/src/scales.rs",
    "clients/desktop/chats.rs",
    "clients/desktop/safety.rs",
    "clients/desktop/slots.rs",
    "clients/desktop/profile.rs",
    "clients/desktop/system_store.rs",
)


def read_texts(root: Path) -> dict[str, str]:
    names = set(NATIVE) | set(CATALOGUES)
    names.update(UI + filename for _, filename, _ in TYPE_DOMAINS)
    for directory in (UI.rstrip("/"), "clients/desktop/prototype"):
        names.update(path.relative_to(root).as_posix()
                     for path in (root / directory).iterdir()
                     if path.suffix in {".js", ".html"})
    return {name: (root / name).read_text() for name in sorted(names)}


def payload(texts: dict[str, str]) -> dict[str, object]:
    sources = {name: source for name, source in texts.items()
               if name.startswith((UI, "clients/desktop/prototype/"))
               if Path(name).suffix in {".js", ".html"}}
    return {
        "sources": sources,
        "domains": native_domains(texts),
        "direct": direct_native_keys(texts),
        "english": json.loads(texts[CATALOGUES[0]]),
        "russian": json.loads(texts[CATALOGUES[1]]),
    }


def desktop_errors(texts: dict[str, str]) -> list[str]:
    result = subprocess.run(
        ["node", str(ROOT / "tools/web-lint/check-translations.mjs")],
        input=json.dumps(payload(texts)), text=True,
        capture_output=True, check=False)
    if result.returncode:
        detail = result.stderr.strip()
        raise ValueError(f"Desktop translation analysis failed: {detail}")
    errors = json.loads(result.stdout)
    if not isinstance(errors, list):
        raise ValueError("Invalid desktop translation checker report")
    if any(not isinstance(error, str) for error in errors):
        raise ValueError("Invalid desktop translation diagnostic")
    return errors
