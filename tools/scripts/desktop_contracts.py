"""Concrete native smoke helper and case-runner interfaces."""

from collections.abc import Mapping, Sequence
from pathlib import Path
import subprocess
from typing import NotRequired, Protocol, TypedDict


class CaseResult(TypedDict):
    case: str
    exit: int
    passed: bool
    metrics: NotRequired[object]


class Notice(TypedDict):
    replaces: int
    summary: str
    body: str
    actions: list[str]


class CaseRunner(Protocol):
    def __call__(
        self,
        name: str,
        extra: Sequence[str],
        profile: str | Path,
        broken: bool = False,
        peer: bool = True,
        timeout: int = 120,
        launcher: Sequence[str] = (),
        extra_env: Mapping[str, str] | None = None,
    ) -> str | None:
        """Run one smoke case and return its optional peer marker."""
        ...


class NativeSmoke(Protocol):
    def display_diagnostics(self) -> str:
        """Describe bounded display state."""
        ...

    def unlock_keyring(self) -> None:
        """Unlock the isolated provider when needed."""
        ...

    def run(
        self,
        arguments: Sequence[str | Path],
        timeout: int = 60,
        env: Mapping[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        """Run a bounded text subprocess."""
        ...

    def read_varint(self, data: bytes, offset: int) -> tuple[int, int]:
        """Parse a bounded signed-profile field."""
        ...

    def signed_profile_payload(self, record_hex: str) -> bytes:
        """Extract the signed payload."""
        ...

    def check_profile_readback(
        self,
        node: Path,
        seed: str,
        peer_id: str,
        env: Mapping[str, str],
        storage: Path,
        hidden: bool = False,
    ) -> bytes:
        """Check local and published profile versions."""
        ...

    def forge_search_signature(self, node_storage: Path) -> None:
        """Corrupt one isolated fixture record."""
        ...

    def expire_sticker_media(self, node_storage: Path, owner: str) -> None:
        """Quarantine isolated expired media."""
        ...

    def video_smoke(self, case: CaseRunner, reports: Path) -> None:
        """Run both raw video smoke sizes."""
        ...

    def start_smoke_node(
        self, node: Path, env: Mapping[str, str], seed: str, log: Path
    ) -> subprocess.Popen[bytes]:
        """Start one isolated native smoke node."""
        ...
