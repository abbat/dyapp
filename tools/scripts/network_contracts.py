"""Consumed CLI records and network scenario observations."""

from collections.abc import Mapping, Sequence
from re import Pattern
import subprocess
from typing import Literal, NotRequired, Protocol, TypeAlias, TypedDict

Peer: TypeAlias = tuple[str, str]


class MediaDetails(TypedDict):
    keep_record: str
    secret: str
    owner: str
    hash: str


class Reply(TypedDict, total=False):
    status: str
    roles: list[str]
    peer_id: str
    record: str
    secret: str
    mailbox: str
    id: str
    ids: list[str]
    more: bool
    pushed: list[str]
    holders: list[str]
    peers: list[str]
    providers: list[str]
    keep: str
    put: str
    get: str
    data: str
    username: str
    password: str
    urls: list[str]
    details: MediaDetails
    missing: list[str]


class PeerCase(TypedDict):
    peer: str
    health: bool
    roundtrip: bool
    holder_placement: bool
    mailbox: bool
    media: bool
    turn: bool


class NetworkCase(TypedDict):
    routing: bool
    relays: list[str]
    holders: list[str]
    flood: dict[str, int]
    acked_on: list[str]
    repaired_on: list[str]
    holders_after_churn: NotRequired[list[str]]


class NetworkHarness(Protocol):
    ANSI: Pattern[str]

    def call(
        self,
        client: str,
        command: Literal["info", "sign-profile", "publish", "get"],
        *args: str,
    ) -> Reply:
        """Read one rehearsal command reply."""
        ...

    def start_node(
        self, bin_dir: str, environment: Mapping[str, str]
    ) -> subprocess.Popen[bytes]:
        """Start one captured loopback node."""
        ...

    def healthy(self, client: str, addr: str) -> bool:
        """Check the node's store role."""
        ...

    def addresses(self, host: str, port: int) -> Peer:
        """Return TCP and QUIC addresses."""
        ...

    def dyappd(self, bin_dir: str, env: Mapping[str, str], *args: str) -> str:
        """Run a storage command."""
        ...

    def stop(
        self, processes: Sequence[subprocess.Popen[bytes]], failed: bool
    ) -> None:
        """Stop every captured node."""
        ...


class MemorySample(TypedDict):
    elapsed_seconds: float
    rss_kib: list[int]


class MemoryNode(TypedDict):
    node: int
    first_peak_kib: int
    second_peak_kib: int
    failed: bool


class MemoryReport(TypedDict):
    duration_limit_seconds: int
    measured_seconds: float
    growth_limit: float
    binary_sha256: str
    rounds: int
    nodes: list[MemoryNode]
    samples: Sequence[MemorySample]
    failed_nodes: list[int]
