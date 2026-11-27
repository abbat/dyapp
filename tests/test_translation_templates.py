"""Native format witnesses and refusal cases for shared text templates."""

from pathlib import Path
import runpy
import unittest

MODULE = runpy.run_path(
    str(
        Path(__file__).resolve().parents[1]
        / "tools/scripts/translation_templates.py"
    )
)
RENDER = MODULE["render"]


class TranslationTemplates(unittest.TestCase):
    def test_native_positional_witnesses_and_language_reordering(self) -> None:
        arguments = {"name": "string", "count": "integer"}
        bindings = {
            "android": {"name": "%1$s", "count": "%2$d"},
            "apple": {"name": "%1$@", "count": "%2$lld"},
            "desktop": {"name": "{peer}", "count": "{total}"},
        }
        english = "For {name}: {count}"
        reordered = "{count}: for {name}"
        for platform, tokens in bindings.items():
            with self.subTest(platform=platform):
                self.assertEqual(
                    RENDER(english, arguments, platform, tokens),
                    f"For {tokens['name']}: {tokens['count']}",
                )
                self.assertEqual(
                    RENDER(reordered, arguments, platform, tokens),
                    f"{tokens['count']}: for {tokens['name']}",
                )
        self.assertEqual(
            RENDER(
                "{count}", {"count": "integer"}, "apple", {"count": "%1$d"}
            ),
            "%1$d",
        )

    def test_unicode_percent_repetition_and_literal_braces(self) -> None:
        arguments = {"peer": "string"}
        prefix = "\u0414\u043b\u044f"
        text = prefix + " {peer}: 100% {{JSON}} {peer}"
        for platform, token in (
            ("android", "%1$s"),
            ("apple", "%1$@"),
            ("desktop", "{name}"),
        ):
            with self.subTest(platform=platform):
                percent = "%" if platform == "desktop" else "%%"
                self.assertEqual(
                    RENDER(text, arguments, platform, {"peer": token}),
                    f"{prefix} {token}: 100{percent} {{JSON}} {token}",
                )
        for platform in ("android", "apple", "desktop"):
            self.assertEqual(
                RENDER("100% {{JSON}}", {}, platform, {}), "100% {JSON}"
            )

    def test_template_expressions_missing_args_and_invalid_types_are_refused(
        self,
    ) -> None:
        for text, arguments in (
            ("", {}),
            ("  ", {}),
            ("bad\0text", {}),
            ("{unknown}", {"name": "string"}),
            ("{name}", {"name": "string", "lost": "integer"}),
            ("{}", {}),
            ("{name!r}", {"name": "string"}),
            ("{name:>10}", {"name": "string"}),
            ("{name.real}", {"name": "string"}),
            ("{name[0]}", {"name": "string"}),
            ("{name", {"name": "string"}),
            ("value", {"bad.name": "string"}),
            ("{name}", {"name": "float"}),
        ):
            with self.subTest(text=text, arguments=arguments):
                with self.assertRaises(ValueError):
                    RENDER(text, arguments, "desktop", {})

    def test_native_types_positions_and_platforms_are_refused(self) -> None:
        for platform, arguments, tokens in (
            ("unknown", {}, {}),
            ("android", {"name": "string"}, {}),
            ("android", {}, {"extra": "%1$s"}),
            ("android", {"name": "string"}, {"name": "%1$d"}),
            ("apple", {"n": "integer"}, {"n": "%1$@"}),
            ("android", {"n": "integer"}, {"n": "%d"}),
            ("apple", {"name": "string"}, {"name": "%@"}),
            ("android", {"name": "string"}, {"name": "%2$s"}),
            ("apple", {"n": "integer"}, {"n": "%0$d"}),
            ("desktop", {"name": "string"}, {"name": "{x.y}"}),
            (
                "android",
                {"a": "string", "b": "integer"},
                {"a": "%1$s", "b": "%1$d"},
            ),
            (
                "desktop",
                {"a": "string", "b": "string"},
                {"a": "{x}", "b": "{x}"},
            ),
        ):
            text = " ".join("{" + name + "}" for name in arguments) or "text"
            with self.subTest(platform=platform, tokens=tokens):
                with self.assertRaises(ValueError):
                    RENDER(text, arguments, platform, tokens)

    def test_native_literal_braces_cannot_reinterpret_an_argument(
        self,
    ) -> None:
        with self.assertRaisesRegex(ValueError, "Literal braces"):
            RENDER(
                "{{x}} {name}", {"name": "string"}, "desktop", {"name": "{x}"}
            )


if __name__ == "__main__":
    unittest.main()
