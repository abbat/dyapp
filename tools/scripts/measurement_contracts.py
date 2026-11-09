"""Desktop inventory and process benchmark observations."""

from collections.abc import Mapping
from typing import NotRequired, TypeAlias, TypedDict

Attributes: TypeAlias = dict[str, str | None]
Conditions: TypeAlias = dict[str | None, list[tuple[str, str]]]


class Control(TypedDict):
    screen: str
    id: str | None
    kind: str
    key: str | None
    label: Attributes | None
    hidden: str
    disabled: str


class Host(TypedDict):
    hostname: str
    uname: Mapping[str, str]
    source_commit: str
    os_release: dict[str, str]
    gpu: list[dict[str, str]]
    webkitgtk: str
    cpu: str
    display: dict[str, str]
    clock_uncertainty: str


ProcessTree: TypeAlias = dict[tuple[int, int], tuple[int, int]]


class ProcessSample(TypedDict):
    seconds: float
    rss_mb: float
    processes: int


class VideoCase(TypedDict):
    frame_metrics: object
    duration_seconds: float
    cpu_seconds: float
    cpu_cores: float
    rss_first_mb: float | None
    rss_last_mb: float | None
    rss_peak_mb: float | None
    samples: list[ProcessSample]
    additional_cpu_cores: NotRequired[float]
