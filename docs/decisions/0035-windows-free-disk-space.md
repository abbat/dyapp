# 0035. Windows free disk space

- **Status:** Accepted
- **Date:** 2026-10-30

## Context

[Desktop storage and status](../design/v0.0.1-client-screens.md#desktop-storage-and-status) shows
the free space on the volume of the message store. Linux and macOS read it with `statvfs` in
`dyapp-bootstrap`. Windows needs `GetDiskFreeSpaceExW`, but the Windows client forbids
`unsafe_code` and does not link `dyapp-bootstrap`, so until now it hid the row.

## Decision

The Windows client depends on `sysinfo` (Windows only, `disk` feature, pinned like the other desktop
dependencies) and takes the available space of the disk whose mount point is the longest prefix
of the identity's slot folder. `sysinfo` calls `GetDiskFreeSpaceExW` behind a safe API.

## Consequences

- The Storage card shows free space on Windows; when no disk matches, the row stays hidden.
- `sysinfo` is already in the Windows dependency tree through libp2p, so no crate is added; only
  its disk code is compiled in.
- The client keeps `unsafe_code = "forbid"`.
