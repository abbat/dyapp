# 0044. Parse sticker manifests in the shared Rust core

- **Status:** Accepted
- **Date:** 2026-11-06

## Context

The [sticker design](../design/stickers.md) requires bounded format-1 JSON,
display-safe pack names and one-grapheme labels. File and digest checks cannot
prove that decoded entries match the original JSON bytes. Each platform parsing
its own copy would duplicate a trust boundary.

## Decision

Use serde 1 with derive and serde_json 1 for typed manifest decoding, and
unicode-segmentation 1 for extended grapheme counting in `dyapp-client`.
Reuse existing locked versions where available. Limit original input to 64 KiB,
packs to 1..120 entries and clean NFC names and labels to 64 code points.
Names must not be blank; labels must contain exactly one nonblank grapheme.
Reject duplicate known fields and malformed SHA-256 hashes; an unknown format
produces an update-required error. Apply the shared file membership/hash checks
to parsed entries. Original manifest bytes define identity, without JSON
reserialization. JSON and Unicode validation remain separate from WebP decoding.

## Consequences

Maintained parsers and Unicode tables provide shared deterministic behavior.
The dependencies require root, Linux, Windows and fuzz lock updates and CI checks
for binary growth and dependency safety. These checks remain release conditions.

Platform parsers could reuse native facilities, but would duplicate validation
and allow platform differences. Hand-written JSON or grapheme segmentation has
disproportionate complexity and correctness cost. Neither alternative is selected.
This decision does not approve the separate proposed image-webp decoder dependency.
