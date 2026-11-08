# Documentation

> **Status:** derived from `Makefile` and the repository tree on 2026-10-07. CI builds no
> documentation and nothing is published: the docs are developer-only and the repository is
> private. Generating or publishing docs (GitHub Pages, DocC, Dokka) is planned only once there are
> user-facing or external-developer docs; note that on a private repository a Pages site is public
> unless the plan is Enterprise Cloud.

## What exists

| Output | Source | State |
|--------|--------|-------|
| Markdown docs | `docs/` (index: [docs/README.md](../README.md)) | Written by hand; read on GitHub or in a clone. |
| Rust API | `make doc` → `cargo doc --workspace --all-features --locked --no-deps` in the offline dev container, with warnings denied | All 6 crates, public items. Output stays in the dev container's workspace cache volume. `make quality` runs the same strict check. |
| Swift / Kotlin API | none | No DocC catalog, no Dokka plugin. |
| Link check | none | Markdown links are not checked by any script or CI job. |

## What belongs in docs/

The reader is an AI agent. A page stays only if an agent needs it to make a decision or do the
work: current behaviour, contracts, rules, commands, and the FIX anchors that check output points
to. Research, audits, investigations, design reviews, reference surveys and finished migrations
go to a bead (or the issue) and leave `docs/` when their result has landed in code, a contract page
or an ADR. Review files keep open findings only; closed ones live in git history. An ADR is
written only when an agent without it would make a wrong decision
([triggers](../decisions/README.md#triggers)).

## Local use

```bash
make prepare   # once, online: builds the dev image (and the other test images)
make doc       # checks that rustdoc builds for all 6 crates; output stays in the container volume
```

To browse the Rust API on the host, copy `/workspace/build/doc` from the retained dev container
with `docker cp` into a new directory under `build/`, then open its `index.html`. A host Rust
toolchain is not required.

Check relative Markdown links by hand when moving files.

## Writing API docs

- **Rust:** `///` on public items, with `# Errors` / `# Panics` / `# Examples` sections where they
  apply. Doc examples are compiled and run by `cargo test` (doctests), so keep them building.
- **Swift / Kotlin:** `///` and KDoc respectively. The platform shells currently have almost no
  public API, so there is little to document until the Rust bindings exist
  ([ffi-bindings.md](ffi-bindings.md)).
