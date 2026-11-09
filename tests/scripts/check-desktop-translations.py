"""Reject unused desktop catalogue keys using the shared offline checker."""

from check_wrapper import wrap
from desktop_translations import ROOT, desktop_errors, read_texts


def main() -> None:
    errors = desktop_errors(read_texts(ROOT))
    if errors:
        raise ValueError("\n".join(errors))
    print("Desktop translation keys: references and en/ru parity verified")


if __name__ == "__main__":
    wrap("web-translations")
    main()
