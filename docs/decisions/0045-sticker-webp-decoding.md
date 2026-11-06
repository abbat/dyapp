# 0045. Decode sticker WebP in the shared Rust core

- **Status:** Accepted
- **Date:** 2026-11-06

## Context

Sticker imports and peer attachments are untrusted. The
[sticker design](../design/stickers.md) chooses static or animated WebP with
first-frame display on every platform. OS decoders would give different
validation and expose peer bytes to multiple implementations.

## Decision

Add image-webp 0.2.4 as a direct dyapp-client dependency. Validate the bounded
RIFF container, canvas and every frame before constructing the decoder.
Recheck decoder dimensions, frame count, duration and output size; set a
16 MiB decoder memory limit. Decode the first frame under catch_unwind,
reject fully transparent output and return bounded RGBA only. Keep the
256 KiB, 512 px, 90-frame and 3-second limits from the sticker design.

## Consequences

One decoder serves Linux/Windows and later native clients. It avoids new OS
decoder paths, but adds dependency/code size and requires root, Linux,
Windows and fuzz lock updates. Some decoder allocations ignore its memory
limit; header validation and fuzzing remain mandatory. CPU, final binary
growth and representative animated-file size must be measured in CI before
release. This is not process isolation and cannot contain an allocator abort
or a decoder hang.

The alternative is OS WebP decoding: less core code, but duplicated trust
checks and inconsistent platform behavior; it conflicts with the current
sticker design. Playback and additional codecs are outside this decision.
