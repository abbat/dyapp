#!/usr/bin/env python3
"""Capacity model for the profile search role (docs/architecture/search-capacity.md)."""
import argparse, json, math

P = {
    "steps": [1e3, 1e4, 1e5, 1e6, 1e7, 5e7],
    "activity": {"base": {"dau": 0.3, "queries": 10, "edit_days": 7},
                 "stress": {"dau": 0.5, "queries": 30, "edit_days": 1}},
    "devices": 1.5, "heartbeats_per_key_max": 2, "beats_per_day": 1, "peak": 3,
    "node_share": [0.001, 0.005, 0.01], "search_share": [0.1, 0.5, 1.0],
    "link": 100e6 / 8, "max_bytes": 50e9, "profile": 5e3, "heartbeat": 200,
    "D": 6,  # gossip copies of one message; the verdict uses D, c1 and c3 are shown for comparison
    "page": 50, "key_version": 40, "sync_peers": 3,
    "top_country": 0.30, "top_place": 0.09, "places": 10_000,
    "select": {"place": 1.0, "typical": 0.11, "rare": 0.01, "country_rare": 0.0025, "in8": 0.11},
    "max_scan": 10_000, "bytes_per_row": 300, "qps": 500,
    # phone: calibrated so 11 bits = 30 s per bootstrap.md (30000 ms / 2^11 tries); 11 bits is the ceiling
    "argon_ms": {"pc_core": 5, "phone": 30000 / 2048},
    "ssd_read_ms": 0.1, "ram": 4e9, "attacker_cores": 16, "pow_bits": [11, 14, 17, 20],
    "peers": 50, "peer_rps": 100, "group_rps": 1000,
}
LIMITS = {"gossip": 0.30, "inventory": 0.10, "get": 0.30, "qps": 0.50, "sync_h": 24,
          "nodes_per_query": 3, "min_search_nodes": 2, "phone_s": 30, "partial": 50}
# Verdict-free classes: a `country + rare` partial is expected (spec), so it is reported, not failing.
EXPECTED_PARTIAL = {"country_rare"}


def zipf(n, top):
    """Shares of n ranks with share[0] == top, by bisection on the exponent."""
    lo, hi = 0.0, 5.0
    for _ in range(100):
        s = (lo + hi) / 2
        w = [1 / (r ** s) for r in range(1, n + 1)]
        if w[0] / sum(w) < top:
            lo = s
        else:
            hi = s
    t = sum(w)
    return [x / t for x in w]


def partial_model(p):
    """Per class: expected matches within max_scan_rows, and whether that is below one page."""
    return {k: (p["max_scan"] * s, p["max_scan"] * s < LIMITS["partial"]) for k, s in p["select"].items()}


def nodes_needed(p, total, sample):
    """Nodes with independent uniform samples a client asks to see a full page, or 95 % of the
    matches when the whole network holds less than a page."""
    if sample >= 1 or total <= 0:
        return 1
    target = min(p["page"], 0.95 * total)
    if target >= total:
        return math.inf
    return max(1, math.ceil(math.log(1 - target / total) / math.log(1 - sample)))


def row(p, users, act, node_share, search_share, places):
    a = p["activity"][act]
    D = p["D"]
    dau = users * a["dau"]
    nodes = max(1.0, dau * node_share)
    search = nodes * search_share
    per_key = p["profile"] + p["bytes_per_row"]
    sample = min(1.0, p["max_bytes"] / (users * per_key))
    held = users * sample
    edits = held * p["profile"] / (a["edit_days"] * 86400)
    beats = (dau * sample * min(p["devices"], p["heartbeats_per_key_max"]) * p["beats_per_day"]
             * p["heartbeat"] / 86400)
    q_node = dau * a["queries"] * p["peak"] / 86400 / max(search, 1e-9)
    scan = {k: p["page"] / s for k, s in p["select"].items()}
    # Cold cache: once the index file exceeds half of RAM, each scanned row costs a disk read.
    index = held * p["bytes_per_row"]
    cold = index > p["ram"] / 2
    cold_ms = scan["typical"] * p["ssd_read_ms"]
    cold_qps = 2 * 1000 / cold_ms
    qps_cap = min(p["qps"], cold_qps) if cold else p["qps"]
    sync_bps = LIMITS["gossip"] * p["link"]
    r = {
        "users": users, "dau": dau, "nodes": nodes, "search": search, "sample": sample,
        "disk": held * per_key, "index": index, "cold": cold, "cold_ms": cold_ms,
        "cold_qps": cold_qps, "qps_cap": qps_cap, "scan": scan,
        "gossip": {c: (edits + beats) * c for c in (1, 3, D)},
        "inventory": held * p["key_version"] / 3600,
        "sync_h": held * per_key / sync_bps / 3600,
        "sync_peer": sync_bps / p["sync_peers"],  # sync runs at the cap, spread over the peers pulled from
        "q_node": q_node, "get": q_node * p["page"] * p["profile"],
        "coverage": {rank: users * p["top_country"] * places[rank - 1] * sample * p["select"]["typical"]
                     for rank in (1, 100, 10_000)},
        "nodes_per_query": {(cls, rank): nodes_needed(p, users * p["top_country"] * places[rank - 1]
                                                      * p["select"][cls], sample)
                            for cls in ("typical", "rare") for rank in (1, 100, 10_000)},
    }
    # Coverage rank 10 000 and the `country_rare` partial are reported, not failing: the spec calls
    # that partial expected, and a place of rank 10 000 is the long tail the report describes.
    fails = []
    if search < LIMITS["min_search_nodes"]:
        fails.append("search nodes")
    if r["gossip"][D] > LIMITS["gossip"] * p["link"]:
        fails.append("gossip" + (" (holds at 3: IDONTWANT)" if r["gossip"][3] <= LIMITS["gossip"] * p["link"] else ""))
    if r["inventory"] > LIMITS["inventory"] * p["link"]:
        fails.append("inventory")
    if r["sync_h"] > LIMITS["sync_h"]:
        fails.append("first sync")
    if q_node > LIMITS["qps"] * qps_cap:
        fails.append("queries" + (" (cold cache)" if cold and cold_qps < p["qps"] else ""))
    if r["get"] > LIMITS["get"] * p["link"]:
        fails.append("get")
    if r["nodes_per_query"][("typical", 100)] > LIMITS["nodes_per_query"]:
        fails.append("coverage rank 100")
    if any(f for k, (_, f) in partial_model(p).items() if k not in EXPECTED_PARTIAL):
        fails.append("partial")
    r["fails"] = fails
    return r


def threshold(rs):
    """(first holding step, first failing step above it with its resource); (None, None) if none holds."""
    h = next((i for i, r in enumerate(rs) if not r["fails"]), None)
    if h is None:
        return None, None
    f = next((r for r in rs[h:] if r["fails"]), None)
    return rs[h], f


def adversary(p):
    out = []
    per_key = p["profile"] + p["bytes_per_row"]
    for bits in p["pow_bits"]:
        tries = 2 ** bits
        keys_day = p["attacker_cores"] * 86400e3 / (p["argon_ms"]["pc_core"] * tries)
        fill_days = p["max_bytes"] / per_key / keys_day
        phone_s = p["argon_ms"]["phone"] * tries / 1e3
        evict = keys_day * per_key / per_key  # fake bytes/day / bytes of one real profile (same size)
        out.append((bits, keys_day, fill_days, phone_s, phone_s <= LIMITS["phone_s"], evict))
    return out


def self_check(p):
    places = zipf(p["places"], p["top_place"])
    near = lambda a, b, tol=0.01: abs(a - b) <= tol * abs(b)
    assert abs(sum(places) - 1) < 1e-9 and abs(places[0] - p["top_place"]) < 1e-3
    r = row(p, 1e6, "base", 0.005, 1.0, places)
    assert abs(r["disk"] - 1e6 * 5300) < 1, r["disk"]
    assert near(r["gossip"][1], 8267.2 + 1041.67), r["gossip"][1]  # edits + heartbeats, B/s
    assert near(r["inventory"], 11111.1), r["inventory"]
    assert near(r["sync_h"], 0.3926), r["sync_h"]
    assert near(r["q_node"], 0.06944, 0.001) and near(r["get"], 17361, 0.001), (r["q_node"], r["get"])
    assert not r["cold"] and r["qps_cap"] == p["qps"] and not r["fails"], r["fails"]
    cold = row(p, 1e7, "stress", 0.005, 1.0, places)
    assert near(cold["sample"], 0.9434, 0.001) and cold["cold"], cold["sample"]
    assert near(cold["cold_ms"], 45.4545) and near(cold["cold_qps"], 44.0) and cold["qps_cap"] == cold["cold_qps"]
    slow = row(p, 1e7, "stress", 5e-6, 1.0, places)  # 25 search nodes: 208 q/s per node > 0.5 * 44 cold
    assert near(slow["q_node"], 208.3, 0.001) and "queries (cold cache)" in slow["fails"], slow["fails"]
    prev = None
    for users in p["steps"]:
        cur = row(p, users, "stress", 0.005, 1.0, places)
        if prev:
            assert cur["disk"] >= prev["disk"] and cur["gossip"][p["D"]] >= prev["gossip"][p["D"]]
        prev = cur
    assert row(p, 1e3, "base", 0.001, 0.1, places)["fails"][0] == "search nodes"
    # A node holding everyone answers alone; at 20 % sample and 10 matches network-wide,
    # 95 % (9.5) needs ln(0.05) / ln(0.8) = 13.4 -> 14 nodes; 1000 matches need ceil(ln(0.95)/ln(0.8)) = 1.
    assert nodes_needed(p, 10, 1.0) == 1 and nodes_needed(p, 10, 0.2) == 14 and nodes_needed(p, 1000, 0.2) == 1
    a = adversary(p)[0]  # 11 bits
    assert near(a[1], 135000) and near(a[2], 69.88) and near(a[3], 30.0) and a[4] and near(a[5], 135000), a
    assert partial_model(p)["country_rare"][1] and not partial_model(p)["rare"][1]


def report(p, source, measured_partial):
    places = zipf(p["places"], p["top_place"])
    D = p["D"]
    print("# Search capacity model\n")
    print(f"Source: {source}; bytes_per_row = {p['bytes_per_row']}, warm qps = {p['qps']}.\n")
    scan = {k: p["page"] / s for k, s in p["select"].items()}
    print("Rows scanned per query class: " + ", ".join(f"{k} {v:,.0f}" for k, v in scan.items()) + ".\n")
    print(f"## Partial share (max_scan_rows = {p['max_scan']:,.0f}, page = {p['page']})\n")
    print("| Class | Expected matches in max_scan_rows | Partial (model) | Partial (measured) |")
    print("|---|---|---|---|")
    for k, (exp, part) in partial_model(p).items():
        m = measured_partial.get(k)
        print(f"| {k} | {exp:,.1f} | {'1.00' if part else '0.00'}{' (expected)' if part and k in EXPECTED_PARTIAL else ''} "
              f"| {'-' if m is None else f'{m:.2f}'} |")
    print()
    grid = {}
    for act in p["activity"]:
        print(f"## {act.capitalize()} activity\n")
        print(f"| Users | Node % | Search % | Nodes | Search nodes | Disk GB | Sample | Gossip MB/s c1/c3/c{D} "
              "| Inventory MB/s | First sync h | Sync MB/s per peer | Queries/s per node | Qps cap warm/cold "
              "| Get MB/s | Typical matches per node r1/r100/r10k | Nodes per query typical r1/r100/r10k "
              "| Nodes per query rare r1/r100/r10k | Verdict |")
        print("|" + "---|" * 18)
        for ns in p["node_share"]:
            for ss in p["search_share"]:
                rs = [row(p, u, act, ns, ss, places) for u in p["steps"]]
                grid[(act, ns, ss)] = rs
                for r in rs:
                    g, c, n = r["gossip"], r["coverage"], r["nodes_per_query"]
                    cap = f"{p['qps']:.0f}/{r['cold_qps']:.0f}" if r["cold"] else f"{p['qps']:.0f}/–"
                    print(f"| {r['users']:,.0f} | {ns:.1%} | {ss:.0%} | {r['nodes']:,.0f} | {r['search']:,.1f} "
                          f"| {r['disk'] / 1e9:.2f} | {r['sample']:.2f} "
                          f"| {g[1] / 1e6:.3f}/{g[3] / 1e6:.3f}/{g[D] / 1e6:.3f} "
                          f"| {r['inventory'] / 1e6:.3f} | {r['sync_h']:.1f} | {r['sync_peer'] / 1e6:.2f} "
                          f"| {r['q_node']:.2f} | {cap} | {r['get'] / 1e6:.3f} "
                          f"| {c[1]:,.0f}/{c[100]:,.0f}/{c[10_000]:,.0f} "
                          + "".join("| " + "/".join(str(n[(cls, k)]) for k in (1, 100, 10_000)) + " "
                                    for cls in ("typical", "rare")) +
                          f"| {'; '.join(r['fails']) or 'holds'} |")
        print()
    print("## Thresholds\n")
    print("Holds from = lowest step with no failure; first failure above = the next failing step "
          "after it, with the first limiting resource.\n")
    print("| Activity | Node % | Search % | Holds from users | First failure above | Limiting resource "
          "| DAU with search >= 2 | DAU with search >= 3 |")
    print("|---|---|---|---|---|---|---|---|")
    for (act, ns, ss), rs in grid.items():
        h, f = threshold(rs)
        print(f"| {act} | {ns:.1%} | {ss:.0%} | {'never' if h is None else format(h['users'], ',.0f')} "
              f"| {'-' if h is None else format(f['users'], ',.0f') if f else 'none up to 50M'} "
              f"| {f['fails'][0] if f else '-'} | {2 / (ns * ss):,.0f} | {3 / (ns * ss):,.0f} |")
    print("\n## Sensitivity (base activity, node 0.5 %, search 50 %)\n")
    print("| Variation | Holds from users | First failure above | Limiting resource |")
    print("|---|---|---|---|")
    variants = [("baseline", {}), ("search.max_bytes x0.5", {"max_bytes": p["max_bytes"] * 0.5}),
                ("search.max_bytes x2", {"max_bytes": p["max_bytes"] * 2}), ("D = 3", {"D": 3}),
                ("D = 12", {"D": 12}), ("heartbeat 2/day", {"beats_per_day": 2}),
                ("max_scan_rows 5000", {"max_scan": 5000}), ("max_scan_rows 20000", {"max_scan": 20000})]
    for name, kw in variants:
        q = dict(p, **kw)
        h, f = threshold([row(q, u, "base", 0.005, 0.5, places) for u in q["steps"]])
        print(f"| {name} | {'never' if h is None else format(h['users'], ',.0f')} "
              f"| {'-' if h is None else format(f['users'], ',.0f') if f else 'none up to 50M'} "
              f"| {f['fails'][0] if f else '-'} |")
    print("\n## Adversary\n")
    print("| PoW bits | Fake keys/day | Days to fill 50 GB | Phone s per bootstrap | Phone ok "
          "| Real profiles evicted/day once full |")
    print("|---|---|---|---|---|---|")
    for bits, kd, fd, ps, ok, ev in adversary(p):
        print(f"| {bits} | {kd:,.0f} | {fd:.2f} | {ps:.1f} | {'yes' if ok else 'no'} | {ev:,.0f} |")
    print(f"\nQuery rate many peers reach past the per-peer limits: {p['peers']} peers x {p['peer_rps']}/s "
          f"= {p['peers'] * p['peer_rps']:,}/s per node, against a warm cap of {p['qps']}/s "
          f"(IP-group cap {p['group_rps']}/s per group).")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bench")
    ap.add_argument("--self-check", action="store_true")
    args = ap.parse_args()
    self_check(dict(P))  # always on the defaults: the hand numbers assume them
    if args.self_check:
        print("self-check OK")
        return
    p = dict(P)
    source = "defaults (no --bench)"
    measured = {}
    if args.bench:
        with open(args.bench) as f:
            b = json.load(f)
        p["bytes_per_row"], p["qps"] = b["bytes_per_row"], b["qps"]
        measured = b.get("partial") or {}
        source = f"{args.bench}" + ("" if b["extrapolation_ok"] else " (extrapolation unreliable)")
    report(p, source, measured)


if __name__ == "__main__":
    main()
