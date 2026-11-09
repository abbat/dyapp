"""Shared GitHub run/job shapes and tracked CI failure records."""

from typing import NotRequired, TypedDict


class Step(TypedDict):
    name: str
    conclusion: str | None


class Job(TypedDict):
    name: str
    conclusion: str | None
    status: NotRequired[str]
    steps: NotRequired[list[Step] | None]


class RunRef(TypedDict):
    id: int


class RunOutcome(RunRef):
    conclusion: str | None


class RunPage(TypedDict):
    workflow_runs: list[RunRef]


class JobPage(TypedDict):
    jobs: list[Job]


class ScheduledRunPage(TypedDict):
    workflow_runs: list[RunOutcome]


class ScheduledRun(RunOutcome):
    name: str
    event: str
    workflow_id: int
    head_sha: str
    html_url: str


class ListedRun(TypedDict):
    databaseId: int
    workflowName: str
    status: str


class ReportedJob(TypedDict):
    name: str
    conclusion: str | None
    steps: list[Step]


class Issue(TypedDict):
    number: int
    title: str


class FailureView(TypedDict):
    workflowName: str
    jobs: list[ReportedJob]


class RunView(FailureView):
    databaseId: int
    headSha: str
    url: str
    event: str


class FailureState(TypedDict):
    status: str
    description: str
    notes: NotRequired[str | None]


class TrackedFailure(FailureState):
    id: str
