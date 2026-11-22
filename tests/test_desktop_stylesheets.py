"""Keep shared desktop styles ordered and packaged on every Tauri platform."""

from html.parser import HTMLParser
import json
from pathlib import Path
from unittest import TestCase, main

ROOT = Path(__file__).resolve().parents[1]


class Stylesheets(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.names: list[str] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        fields = dict(attrs)
        if tag == "link" and fields.get("rel") == "stylesheet":
            name = fields.get("href")
            if name is not None:
                self.names.append(name)


def styles(name: str) -> list[str]:
    parser = Stylesheets()
    parser.feed((ROOT / name).read_text())
    return parser.names


class DesktopStylesheets(TestCase):
    def test_shared_order_precedes_prototype_overrides(self) -> None:
        self.assertEqual(
            styles("clients/desktop/ui/index.html"),
            ["style.css", "shell.css"],
        )
        self.assertEqual(
            styles("clients/desktop/prototype/index.html"),
            ["../ui/style.css", "../ui/shell.css", "style.css"],
        )

    def test_every_tauri_frontend_packages_both_shared_stylesheets(
        self,
    ) -> None:
        for platform in ("linux", "windows", "macos"):
            with self.subTest(platform=platform):
                config = ROOT / "clients" / platform / "tauri.conf.json"
                value = json.loads(config.read_text())
                assets = (
                    config.parent / value["build"]["frontendDist"]
                ).resolve()
                self.assertEqual(
                    assets, (ROOT / "clients/desktop/ui").resolve()
                )
                for name in ("style.css", "shell.css"):
                    self.assertTrue((assets / name).is_file())


if __name__ == "__main__":
    main()
