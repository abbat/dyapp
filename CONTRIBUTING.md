# Contributing

Thank you for helping. dyapp is written by AI agents under one maintainer, and an agent will
likely answer your issue or review your pull request first. You can always reach the maintainer
(GitHub user abbat) in the same thread.

## Before you start

- **Bugs, ideas, translations, node problems:** open an
  [issue](https://github.com/abbat/dyapp/issues/new/choose). Pick the English or Russian form.
- **Security problems:** do not open a public issue; follow [SECURITY.md](.github/SECURITY.md).
- **Code:** for anything larger than a typo, open an issue first so we can agree on the approach
  before you spend time on it.

Write in English or Russian; replies follow your language.

## How a pull request is reviewed

The process is in [pr-review.md](docs/development/pr-review.md). From your side:

1. CI does not start on its own. GitHub asks the maintainer to approve each run for a fork pull
   request, after reading the diff.
2. Two reviewer agents read the same commit independently. Every finding is answered before the
   merge.
3. Any new commit restarts the review, so push fixes in one go when you can.

## Making changes

All checks run in Docker. Build the image once, then run the checks:

```sh
make prepare   # build the check image
make quality   # linters, formatters, repository checks
make test      # Rust and Python tests
```

The full list of targets and what they run is in
[code-quality.md](docs/development/code-quality.md#local-checks).

Commit rules, checked by `make quality` and CI
([messages](docs/development/code-quality.md#commit-messages),
[size](docs/development/code-quality.md#small-batches)):

- a capitalised subject of 72 characters or less, in the imperative, without a final period,
  then a blank line before the body;
- a `Co-Authored-By: Name <email>` trailer; credit the AI tool you worked with, or yourself;
- at most 400 changed lines per commit, unless a `Large-Change: <reason>` trailer says why;
- a new dependency needs an issue with the reason and an ADR in
  [docs/decisions](docs/decisions/), named in an `ADR: docs/decisions/...` trailer.

Application code, infrastructure (`.github/`, build scripts) and dependency changes go in
separate pull requests.

Never commit keys, recovery phrases, real profiles or other personal data, including in tests
and screenshots.

## License

dyapp is under the [MIT license](LICENSE). By sending a pull request you agree that your change
is published under it. There is no CLA and no sign-off line to add.
