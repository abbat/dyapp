"""Typed records consumed and emitted by repository quality commands."""

from collections.abc import Mapping
from typing import NotRequired, Required, TypeAlias, TypedDict


class SizePolicy(TypedDict):
    max_lines: int
    files: NotRequired[dict[str, int]]


class Rating(TypedDict):
    cost_of_delay: object
    job_size: object
    rationale: str


class Metadata(TypedDict):
    wsjf: NotRequired[Rating]


class WorkItem(TypedDict):
    id: str
    title: str
    status: str
    issue_type: str
    priority: int
    parent: NotRequired[str | None]
    labels: NotRequired[list[str]]
    metadata: NotRequired[Metadata | None]


class RankedItem(TypedDict):
    id: str
    title: str
    cost_of_delay: float
    job_size: float
    wsjf: float
    provisional: bool
    rationale: str
    practices: bool


class Proposal(TypedDict):
    schema: int
    generated_at: str
    proposal_sha256: str
    policy: NotRequired[str]
    order: list[RankedItem]
    approval: str
    errors: NotRequired[list[str]]


class Approval(TypedDict):
    approved_at: str
    approved_by: str
    proposal_sha256: str


class ErrorReport(TypedDict):
    errors: list[str]


class AndroidArtifact(TypedDict):
    path: str
    bytes: int
    sha256: str
    abis: list[str]


class AndroidReport(TypedDict):
    expected_profile: str
    signing: str
    artifacts: dict[str, AndroidArtifact]
    errors: list[str]


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


class PublicLocation(TypedDict, total=False):
    file: str
    line: int


class PublicDiagnostic(TypedDict):
    code: str
    identity: NotRequired[PublicIdentity]
    location: NotRequired[PublicLocation | None]
    message: NotRequired[str]


class PublicReport(TypedDict):
    schema_version: int
    summary: PublicSummary
    diagnostics: list[PublicDiagnostic]


class Alert(TypedDict):
    id: str
    severity: str
    created_at: str
    closed_at: str | None


class NamedAdvisory(TypedDict):
    id: str
    state: str
    names: list[str]


class Advisory(Alert):
    state: str
    names: list[str]


class AdvisoryMetadata(TypedDict):
    ghsa_id: str
    severity: str


class GitHubSecurityRecord(TypedDict):
    created_at: str
    security_advisory: NotRequired[AdvisoryMetadata]
    fixed_at: NotRequired[str | None]
    dismissed_at: NotRequired[str | None]
    ghsa_id: NotRequired[str]
    state: NotRequired[str]
    severity: NotRequired[str]
    closed_at: NotRequired[str | None]
    published_at: NotRequired[str | None]
    cve_id: NotRequired[str | None]


class CompilerSpan(TypedDict):
    file_name: str
    is_primary: bool
    byte_start: NotRequired[int]
    byte_end: NotRequired[int]
    line_start: NotRequired[int]
    column_start: NotRequired[int]


class CompilerCode(TypedDict):
    code: NotRequired[str]


class CompilerMessage(TypedDict, total=False):
    level: str
    message: str
    code: CompilerCode | None
    spans: list[CompilerSpan]


class CompilerTarget(TypedDict):
    kind: list[str]


class CompilerRecord(TypedDict):
    reason: str
    package_id: NotRequired[str]
    target: NotRequired[CompilerTarget]
    message: NotRequired[CompilerMessage]
    success: NotRequired[bool]


class WorkspaceMembers(TypedDict):
    workspace_members: list[str]


class CargoPackage(TypedDict):
    id: str
    name: str


class CargoMetadata(WorkspaceMembers):
    packages: list[CargoPackage]
    target_directory: str


class ParserFunction(TypedDict):
    id: str
    kind: str
    start: list[int]
    end: list[int]
    nesting: int


class ComplexityFinding(TypedDict):
    code: str
    span: CompilerSpan
    message: str


class PanicLocation(TypedDict):
    package: str
    lint: str
    spans: list[tuple[str, int, int]]


class ComplexityBudget(TypedDict):
    path: str
    function: str
    metrics: dict[str, int]


class ComplexityReport(TypedDict):
    scope: str
    errors: list[str]
    functions: NotRequired[list[ComplexityBudget]]


class PanicReport(TypedDict, total=False):
    errors: Required[list[str]]
    counts: dict[str, int]
    budgets: dict[str, int]
    diagnostics: list[PanicLocation]
    excluded_diagnostics: dict[str, int]


FunctionKey: TypeAlias = tuple[str, str]
Measurements: TypeAlias = dict[FunctionKey, dict[str, int]]
MetricMap: TypeAlias = Mapping[FunctionKey, Mapping[str, int]]
DiagnosticKey: TypeAlias = tuple[str, str, int, int, str]
