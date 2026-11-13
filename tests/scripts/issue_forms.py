"""Keep the language copies of each GitHub issue form in step."""

import re

import yaml

FORM = re.compile(r"\.github/ISSUE_TEMPLATE/(.+)-(en|ru)\.yml")
LANGUAGES = ("en", "ru")
Field = tuple[str | None, str, bool, int]


def shape(text: str) -> tuple[list[str], list[Field]]:
    form = yaml.safe_load(text)
    labels = [
        label for label in form["labels"] if not label.startswith("lang:")
    ]
    fields = [
        (
            item.get("id"),
            item["type"],
            item.get("validations", {}).get("required", False),
            len(item.get("attributes", {}).get("options", [])),
        )
        for item in form["body"]
        if item["type"] != "markdown"
    ]
    return labels, fields


def form_errors(texts: "dict[str, str]") -> list[str]:
    forms: dict[str, dict[str, str]] = {}
    for name, text in texts.items():
        if match := FORM.fullmatch(name):
            forms.setdefault(match[1], {})[match[2]] = text
    errors = []
    for stem, copies in sorted(forms.items()):
        missing = [lang for lang in LANGUAGES if lang not in copies]
        if missing:
            errors.append(f"{stem}: no {', '.join(missing)} copy")
            continue
        for lang in LANGUAGES:
            labels = yaml.safe_load(copies[lang])["labels"]
            if f"lang:{lang}" not in labels:
                errors.append(f"{stem}-{lang}: no lang:{lang} label")
            if any(field[0] is None for field in shape(copies[lang])[1]):
                errors.append(f"{stem}-{lang}: a field has no id")
        if shape(copies["en"]) != shape(copies["ru"]):
            errors.append(f"{stem}: en and ru differ in labels or fields")
    return errors
