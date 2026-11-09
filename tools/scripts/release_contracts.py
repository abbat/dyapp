"""Release inventory, feed records and injected command/HTTP interfaces."""

from collections.abc import Sequence
from contextlib import AbstractContextManager
from re import Pattern
from typing import NotRequired, Protocol, TypedDict
from urllib.request import Request


class FeedTarget(TypedDict):
    version: str
    tag: str
    method: str


class Feed(TypedDict):
    schema: int
    channel: str
    targets: dict[str, FeedTarget]


class Asset(TypedDict):
    name: str
    state: str
    sha256: str


class Attestation(TypedDict):
    name: str
    sha256: str


class SourceBuild(TypedDict):
    target: str
    commit: str


class Claim(TypedDict):
    method: str
    asset: NotRequired[str]


class Release(TypedDict):
    tag: str
    commit: str
    draft: bool
    prerelease: bool
    assets: list[Asset]
    attestations: list[Attestation]
    source_builds: list[SourceBuild]
    targets: dict[str, Claim]


class Inventory(TypedDict):
    schema: int
    releases: list[Release]


class Package(TypedDict):
    name: str
    sha256: str
    size: int


class Candidates(TypedDict):
    schema: int
    tag: str
    commit: str
    packages: dict[str, Package]


class CommandRunner(Protocol):
    def __call__(self, args: Sequence[str], timeout: int = 120) -> bytes:
        """Run an external verification command."""
        ...


class CandidateModule(Protocol):
    SHA: Pattern[str]
    MAX_BYTES: int

    def stable_tag(self, tag: object) -> None:
        """Validate the stable release tag."""
        ...

    def unique_object(
        self, pairs: list[tuple[str, object]]
    ) -> dict[str, object]:
        """Reject duplicate JSON keys."""
        ...

    def candidates(self, release: object, tag: str, commit: str) -> Candidates:
        """Extract untrusted candidates."""
        ...


class FeedModule(Protocol):
    def unique_object(
        self, pairs: list[tuple[str, object]]
    ) -> dict[str, object]:
        """Reject duplicate JSON keys."""
        ...

    def build(self, inventory: object, prior: object = None) -> Feed:
        """Validate inventory before merging target claims."""
        ...


class InventoryModule(Protocol):
    def inventory(self, tag: str) -> Inventory:
        """Verify published packages and create feed input."""
        ...


class Response(Protocol):
    def geturl(self) -> str:
        """Return the final response URL."""
        ...

    def read(self, limit: int, /) -> bytes:
        """Read a bounded body."""
        ...


class Opener(Protocol):
    def open(
        self, request: Request, /, *, timeout: int
    ) -> AbstractContextManager[Response]:
        """Open a bounded same-origin response."""
        ...
