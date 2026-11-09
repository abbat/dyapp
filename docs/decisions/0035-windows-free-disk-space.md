# 0035. Windows free disk space

- **Status:** Accepted
- **Date:** 2026-10-30

## Context

[Desktop storage and status](../design/v0.0.1-client-screens.md#desktop-storage-and-status) shows
the free space on the volume of the message store. Linux and macOS read it with `statvfs` in
`dyapp-bootstrap`. Windows needs `GetDiskFreeSpaceExW`, but the Windows client forbids
`unsafe_code` and does not link `dyapp-bootstrap`, so until now it hid the row.

## Decision

Originally the Windows client used `sysinfo` with its disk feature and selected the volume
by the longest matching mount point. With the Windows embedded node
([ADR 0030](0030-desktop-full-node.md)), it now links `dyapp-bootstrap` and uses that crate's
safe `storage::free` API, as Linux does. The Windows implementation calls
`GetDiskFreeSpaceExW` for the directory containing the store and returns bytes available to the
caller, respecting disk quotas. The unsafe call stays inside the bootstrap crate; the desktop
client still forbids unsafe code. The redundant direct `sysinfo` disk dependency is removed.

## Consequences

- The Storage card shows free space on Windows; a failed filesystem query leaves it unavailable.
- `windows-sys` is already in the dependency tree. Bootstrap enables only its filesystem API;
  clients share the same free-space query used by node storage limits.
- The client keeps `unsafe_code = "forbid"`.
