"""Concrete data and callback contracts for repository check fixtures."""

from collections.abc import Callable, Mapping
from datetime import datetime
from pathlib import Path
import subprocess
from typing import NotRequired, Protocol, TypeAlias, TypedDict, Union

JSONValue: TypeAlias = Union[
    None, bool, int, float, str, list["JSONValue"], dict[str, "JSONValue"]
]

ExceptionDates: TypeAlias = Mapping[tuple[str, str], tuple[str, str]]
PublicationLookup: TypeAlias = (
    Callable[[str, str, str], datetime] | Exception | None
)
ProcessResult: TypeAlias = (
    subprocess.CompletedProcess[str] | subprocess.CompletedProcess[bytes]
)


class GapFixture(TypedDict):
    scope: str
    a: str
    b: str
    gap: int
    framed: bool
    identifier: bool
    label: bool


class MutantFunction(TypedDict):
    function_name: str


class Mutant(TypedDict, total=False):
    package: str
    name: str
    file: str
    function: MutantFunction


class MutantScenario(TypedDict):
    Mutant: Mutant


class Outcome(TypedDict):
    scenario: str | MutantScenario
    summary: str


class Shard(TypedDict):
    caught: int
    timeout: int
    missed: int
    unviable: int
    outcomes: NotRequired[list[Outcome]]


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


class TargetClaim(TypedDict):
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
    targets: dict[str, TargetClaim]


class Inventory(TypedDict):
    schema: int
    releases: list[Release]


class PreviousTarget(TypedDict):
    version: str
    tag: str
    method: str


class PreviousFeed(TypedDict):
    schema: int
    channel: str
    targets: dict[str, PreviousTarget]


class GitHubAsset(TypedDict):
    name: str
    digest: str
    state: str
    size: int


class GitHubRelease(TypedDict):
    tag_name: str
    draft: bool
    prerelease: bool
    published_at: str | None
    target_commitish: str
    assets: list[GitHubAsset]


class Alert(TypedDict):
    id: str
    severity: str
    created_at: str
    closed_at: str | None


class QuorumMember(TypedDict):
    issue_type: str
    status: str
    labels: list[str]
    description: NotRequired[str]
    assignee: NotRequired[str]
    close_reason: NotRequired[str]


class Rating(TypedDict):
    cost_of_delay: float
    job_size: float
    rationale: str


class Metadata(TypedDict):
    wsjf: Rating


class WorkItem(TypedDict):
    id: str
    title: str
    issue_type: str
    priority: int
    parent: str | None
    status: str
    labels: NotRequired[list[str]]
    metadata: NotRequired[Metadata]


class AppleSummary(TypedDict):
    totalTestCount: int
    passedTests: int
    failedTests: int
    skippedTests: int
    result: str


class CoverageFile(TypedDict):
    filename: str


class CoverageCounts(TypedDict):
    count: int | str
    covered: int | str


class CoverageTotals(TypedDict):
    lines: CoverageCounts


class CoverageDatum(TypedDict):
    files: list[CoverageFile]
    totals: CoverageTotals


class CoverageReport(TypedDict):
    type: str
    data: list[CoverageDatum]


class DiagnosticCode(TypedDict):
    code: str


class Span(TypedDict):
    file_name: str
    is_primary: bool
    line_start: NotRequired[int]
    column_start: NotRequired[int]
    byte_start: NotRequired[int]
    byte_end: NotRequired[int]


class Diagnostic(TypedDict):
    code: DiagnosticCode
    spans: list[Span]
    message: NotRequired[str]
    level: NotRequired[str]


class CompilerTarget(TypedDict):
    kind: list[str]


class CompilerRecord(TypedDict):
    reason: str
    package_id: NotRequired[str]
    message: NotRequired[Diagnostic]
    target: NotRequired[CompilerTarget]
    success: NotRequired[bool]


class FunctionEntry(TypedDict):
    id: str
    kind: str
    start: list[int]
    end: list[int]
    nesting: int


class Finding(TypedDict):
    code: str
    span: Span
    message: str


class Budget(TypedDict):
    path: str
    function: str
    metrics: dict[str, int | str]


class TestNode(TypedDict):
    nodeType: str
    name: NotRequired[str]
    result: str
    children: NotRequired[list["TestNode"]]


class CommitCount(TypedDict):
    lines: int
    reason: str


class HistoryReport(TypedDict):
    commits: list[CommitCount]
    large_percent: float
    errors: list[str]
    large_commits: int
    total_commits: int


class ManifestRecord(TypedDict, total=False):
    schema: int
    peer_id: str
    network: str
    absent: list[str]
    hot_journal: bool
    key_mode: str
    path: str
    size: int
    sha256: str


class ProcessOptions(TypedDict, total=False):
    stdout: int
    stderr: int
    env: Mapping[str, str]
    cwd: str | Path
    timeout: float | None
    text: bool
    capture_output: bool
    check: bool
    start_new_session: bool


class ByteRunner(Protocol):
    def __call__(self, args: list[str], timeout: int = 120) -> bytes:
        """Return captured command bytes."""
        ...


class ProbeOptions(TypedDict, total=False):
    age_identity: Path
    age_binary: Path


class StorageOptions(ProbeOptions, total=False):
    free: Callable[[Path], int]


class PublicTarget(TypedDict):
    package: str
    binary: NotRequired[str]
    library: NotRequired[str]


class PublicSummary(TypedDict):
    target: str
    diagnostic_count: int
    includes_non_production_targets: bool
    feature_profiles: list[str]
    production: list[PublicTarget]


class PublicIdentity(TypedDict):
    crate: str
    item: str


class PublicLocation(TypedDict):
    file: str
    line: int


class PublicDiagnostic(TypedDict):
    code: str
    identity: NotRequired[PublicIdentity]
    location: NotRequired[PublicLocation]
    message: NotRequired[str]


class PublicReport(TypedDict):
    schema_version: int | None
    summary: PublicSummary
    diagnostics: list[PublicDiagnostic]


class RunnerOptions(TypedDict, total=False):
    payload: bytes
    attestation: JSONValue
    head: str
    state: str
