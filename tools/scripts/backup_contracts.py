"""Validated snapshot records and interfaces of dynamically loaded helpers."""

from collections.abc import Callable
from contextlib import AbstractContextManager
from pathlib import Path
from typing import Protocol, TypeAlias, TypedDict


class Header(TypedDict):
    schema: int
    peer_id: str
    network: str
    key_mode: str
    absent: list[str]
    hot_journal: bool


class FileRecord(TypedDict):
    path: str
    size: int
    sha256: str


class CopiedFile(TypedDict):
    path: Path
    size: int
    sha256: str


Inventory: TypeAlias = tuple[
    Header, int, set[str], list[FileRecord], dict[str, FileRecord]
]
SQLiteResult: TypeAlias = tuple[Header, dict[str, FileRecord], int]
FreeSpace: TypeAlias = Callable[[Path], int]


class BackupModule(Protocol):
    REQUIRED: set[str]
    OPTIONAL: set[str]
    DATABASES: set[str]
    MAX_PATH: int
    MAX_FILES: int
    MAX_RECORD: int
    MAX_MANIFEST: int

    def paths(
        self, source: Path, key_mode: str
    ) -> tuple[list[tuple[str, Path]], list[str]]:
        """List the existing source inventory."""
        ...

    def snapshot(
        self,
        source: Path,
        destination: Path,
        key_mode: str,
        recovery_key: Path | None,
    ) -> None:
        """Copy a stopped node into a new snapshot."""
        ...


class VerifierModule(Protocol):
    BACKUP: BackupModule

    def open_beneath(self, root: Path, relative: str) -> int:
        """Open one relative file without following symlinks."""
        ...

    def check_file(self, root: Path, record: FileRecord) -> None:
        """Verify bytes against the manifest."""
        ...

    def parse_and_hash(self, root: Path) -> Inventory:
        """Validate and collect the snapshot manifest."""
        ...

    def check_inventory(
        self, root: Path, expected_count: int, expected_dirs: set[str]
    ) -> None:
        """Require the exact file and directory inventory."""
        ...


class SQLiteModule(Protocol):
    VERIFY: VerifierModule

    def copy_verified_file(
        self, snapshot: Path, scratch: Path, record: FileRecord
    ) -> None:
        """Copy one record and check its digest."""
        ...

    def probe(
        self, snapshot: Path, scratch: Path | None = None
    ) -> SQLiteResult:
        """Check disposable SQLite copies."""
        ...


class ProbeModule(Protocol):
    SQLITE: SQLiteModule

    def probe(
        self,
        snapshot: Path,
        dyappd: Path,
        id_pow_bits: int,
        age_identity: Path | None = None,
        age_binary: Path | None = None,
        scratch: Path | None = None,
    ) -> str:
        """Verify the recovered key and return its peer ID."""
        ...


class PreflightModule(Protocol):
    PROBE: ProbeModule
    MARGIN: int

    def available(self, path: Path) -> int:
        """Return free filesystem bytes."""
        ...

    def stopped(self, source: Path) -> AbstractContextManager[None]:
        """Hold the node's exclusive storage lock."""
        ...
