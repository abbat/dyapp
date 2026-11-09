"""Consumed work-item, delivery-gate and lockfile records."""

from typing import NotRequired, TypeAlias, TypedDict


class WorkItem(TypedDict):
    id: str
    issue_type: str
    status: str
    description: NotRequired[str | None]
    acceptance_criteria: NotRequired[str | None]
    design: NotRequired[str | None]
    notes: NotRequired[str | None]
    labels: NotRequired[list[str]]
    assignee: NotRequired[str | None]
    close_reason: NotRequired[str | None]


class QuorumReport(TypedDict):
    errors: list[str]
    base: NotRequired[str]
    head: NotRequired[str]
    triggers: NotRequired[list[str]]
    quorum_required: NotRequired[bool]
    checks_tightening: NotRequired[bool]


class LockPackage(TypedDict):
    name: str
    version: str
    source: NotRequired[str | None]
    checksum: NotRequired[str]


PackageKey: TypeAlias = tuple[str, str, str | None]
Packages: TypeAlias = dict[PackageKey, LockPackage]


class ReproducedBuild(TypedDict):
    directory: str
    log: str
    exit_code: int
    sha256: NotRequired[str]
    bytes: NotRequired[int]


class ReproducibilityReport(TypedDict):
    target: str
    revision: str | None
    rustc: str
    lock_sha256: str
    source_date_epoch: str | None
    builds: list[ReproducedBuild]
    errors: list[str]
