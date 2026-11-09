"""Inputs and observations of the existing search capacity model."""

from typing import NotRequired, TypeAlias, TypedDict


class Activity(TypedDict):
    dau: float
    queries: int
    edit_days: int


class Parameters(TypedDict):
    steps: list[float]
    activity: dict[str, Activity]
    devices: float
    heartbeats_per_key_max: int
    beats_per_day: int
    peak: int
    node_share: list[float]
    search_share: list[float]
    link: float
    max_bytes: float
    profile: float
    heartbeat: int
    D: int
    page: int
    key_version: int
    sync_peers: int
    top_country: float
    top_place: float
    places: int
    select: dict[str, float]
    max_scan: int
    bytes_per_row: float
    qps: float
    argon_ms: dict[str, float]
    ssd_read_ms: float
    ram: float
    attacker_cores: int
    pow_bits: list[int]
    peers: int
    peer_rps: int
    group_rps: int
    newcomer_share: float
    newcomer_days: int
    heartbeat_bits: int
    heartbeat_argon_ms: float
    growth_daily: list[float]
    puzzle_hashes_s: dict[str, float]
    puzzle_bits: list[int]
    active_groups: int


class Variation(TypedDict, total=False):
    max_bytes: float
    D: int
    beats_per_day: int
    max_scan: int


class CapacityRow(TypedDict):
    users: float
    dau: float
    nodes: float
    search: float
    sample: float
    disk: float
    index: float
    cold: bool
    cold_ms: float
    cold_qps: float
    qps_cap: float
    scan: dict[str, float]
    gossip: dict[int, float]
    inventory: float
    sync_h: float
    sync_peer: float
    q_node: float
    get: float
    coverage: dict[int, float]
    nodes_per_query: dict[tuple[str, int], float]
    fails: NotRequired[list[str]]


Grid: TypeAlias = dict[tuple[str, float, float], list[CapacityRow]]
Adversary: TypeAlias = tuple[int, float, float, float, bool, float]


class Defences(TypedDict):
    new_fakes: float
    fake_share: float
    full_fake_share: float
    heartbeat_s: float
    sustainable_fakes: float
    quota_growth: float
    growth: dict[float, float]
    place_ceiling: float
    rare_profiles: float
    rare_sample: float
    rare_nodes: float
    full: bool


class Cell(TypedDict):
    queries: int
    p50_ms: float
    p99_ms: float
    rows_scanned: float | None
    found_mean: float
    partial: float
    plan: str
    temp_btree: bool


class BenchmarkRun(TypedDict, total=False):
    build_s: dict[str, float]
    db_bytes: int
    bytes_per_row: float
    classes: dict[str, dict[str, Cell]]
    qps_typical_2threads: float
    rss_kb: int


class Benchmark(TypedDict):
    sqlite: str
    runs: dict[str, BenchmarkRun]
    partial_run: bool
    bytes_per_row: NotRequired[float]
    qps: NotRequired[float]
    error_2_5m: NotRequired[float | None]
    extrapolation_ok: NotRequired[bool]
    fit_typical_p50_ms: NotRequired[dict[str, float]]
