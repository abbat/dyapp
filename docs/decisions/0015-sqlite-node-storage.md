# 0015. SQLite for bootstrap node storage

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

The bootstrap store used RocksDB. Its build needs clang and libclang, adds minutes to every clean
build and a large native library to the node. A node holds profiles, mailboxes, signals and a
search index ([bootstrap.md](../architecture/bootstrap.md#target-design--planned)). The search
index needs secondary indexes, which RocksDB has no built-in support for. Operators need a backup
and inspection tool they already know. A design change must never stop a node from serving: a new
store or index format is built next to the old one while the node runs.

## Decision

Store node data in SQLite (`rusqlite`, bundled build), one file per data type, in WAL mode with
`synchronous = NORMAL` and `auto_vacuum = INCREMENTAL`. Media blobs stay files on disk, not rows.
No transaction spans two files, so each store can be rebuilt, migrated or dropped on its own.

## Consequences

- No clang in the build image; a C compiler is enough. `sqlite3` works for backups
  (`.backup`) and inspection.
- SQL indexes cover search filters without a hand-written index layer.
- One writer per file: writes to one store are serialised. The code keeps one connection per file
  behind a mutex; a reader pool comes when reads contend.
- WAL growth and free pages need scheduled checkpoints and incremental vacuum under an I/O budget
  instead of a routine full `VACUUM`.
- No migration from existing RocksDB data: there were no deployed nodes.
