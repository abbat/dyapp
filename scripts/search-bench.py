#!/usr/bin/env python3
"""SQLite measurement for the profile search index (docs/architecture/search-capacity.md).

Builds a synthetic profiles-idx.db per --rows value, runs the design's search query
(`conds AND r >= s ORDER BY r LIMIT 50`, wrap-around, scan cap 10 000) per query class and
place rank, and writes timings, plans and the 10M extrapolation inputs as JSON.
One-off study: stdlib only, no CI. Identifiers are backtick-quoted; runtime values are bound.
"""
import argparse, json, math, os, random, resource, shutil, sqlite3, statistics, sys, threading, time

PAGE, CAP, BUDGET_S = 50, 10_000, 600
MINR, MAXR = -(2**63), 2**63 - 1
T0 = time.monotonic()


class OutOfTime(Exception):
    pass


def guard():
    if time.monotonic() - T0 > BUDGET_S:
        raise OutOfTime


def say(msg):
    print(f"[{time.monotonic() - T0:7.1f}s] {msg}", flush=True)


# ---- data model -------------------------------------------------------------------------------
COUNTRIES = "RU US DE BR IN FR GB IT ES UA PL TR NL CA AU MX AR SE CZ PT".split()
PLACES = 10_000  # places per country; the top 1 000 get their own weight row, the rest one tail row
TOP = 1_000

# string enums: column -> [(code, slots out of the column's total)]; codes are 3 chars
ENUMS = {
    "relationship_goal": [("lng", 3), ("sht", 3), ("frd", 2), ("unk", 2)],
    "career_ambition": [("low", 3), ("mid", 4), ("hgh", 3)],
    "education": [("ed0", 2), ("ed1", 3), ("ed2", 3), ("ed3", 2)],  # ed1 = 30 %
    "orientation": [("str", 5), ("gay", 3), ("bis", 2)],  # str = 50 %
    "role_preference": [("rl0", 4), ("rl1", 3), ("rl2", 3)],
    "zodiac": [(f"zd{i:x}", 1) for i in range(12)],
    "financial_philosophy": [("fn0", 3), ("fn1", 3), ("fn2", 2), ("fn3", 2)],
    "employment_status": [("em0", 4), ("em1", 3), ("em2", 2), ("em3", 1)],
    "fitness_level": [("fl0", 5), ("fl1", 5)],  # fl1 = 50 %
}

SCHEMA = """
CREATE TABLE `profile` (`key` BLOB PRIMARY KEY, `version` INTEGER, `deleted` INTEGER, `age` INTEGER,
  `country` TEXT, `income_from` INTEGER, `income_to` INTEGER, `has_kids` INTEGER,
  `wants_more_kids` INTEGER, `relationship_goal` TEXT, `career_ambition` TEXT, `education` TEXT,
  `orientation` TEXT, `role_preference` TEXT, `zodiac` TEXT, `financial_philosophy` TEXT,
  `employment_status` TEXT, `fitness_level` TEXT, `place` TEXT, `heartbeat` INTEGER, `r` INTEGER)
  WITHOUT ROWID;
CREATE TABLE `interest` (`key` BLOB, `interest` TEXT, PRIMARY KEY (`key`, `interest`)) WITHOUT ROWID;
CREATE TABLE `place_weight` (`lo` REAL PRIMARY KEY, `hi` REAL, `country` TEXT, `place` TEXT);
"""
INDEXES = [
    "CREATE INDEX `idx_place` ON `profile` (`country`, `place`, `r`)",
    "CREATE INDEX `idx_country` ON `profile` (`country`, `r`)",
    "CREATE INDEX `idx_r` ON `profile` (`r`)",
]


def zipf_exponent(n, top_share):
    """Exponent s with 1 / sum(k^-s, k<=n) = top_share (bisection; the sum falls as s grows)."""
    lo, hi = 0.0, 4.0
    for _ in range(60):
        s = (lo + hi) / 2
        share = 1 / sum(k ** -s for k in range(1, n + 1))
        lo, hi = (s, hi) if share < top_share else (lo, s)
    return (lo + hi) / 2


def place_weights():
    """Cumulative [lo, hi) intervals: countries Zipf (top 30 %), places Zipf (top 9 %)."""
    sc, sp = zipf_exponent(len(COUNTRIES), 0.30), zipf_exponent(PLACES, 0.09)
    cw = [k ** -sc for k in range(1, len(COUNTRIES) + 1)]
    pw = [k ** -sp for k in range(1, PLACES + 1)]
    ctot, ptot = sum(cw), sum(pw)
    rows, acc = [], 0.0
    for c, w in zip(COUNTRIES, cw):
        for k in range(TOP):
            share = w / ctot * pw[k] / ptot
            rows.append((acc, acc + share, c, f"place-{k + 1}"))
            acc += share
        share = w / ctot * sum(pw[TOP:]) / ptot
        rows.append((acc, acc + share, c, None))  # tail bucket: place drawn at random from the rest
        acc += share
    rows[-1] = rows[-1][:1] + (2.0,) + rows[-1][2:]  # cover float error at the top
    return rows


def fill_sql():
    enum_cols = list(ENUMS)
    enum_exprs = ", ".join(
        f"substr(:{c}, 1 + 3 * (abs(random()) % {sum(n for _, n in ENUMS[c])}), 3)" for c in enum_cols
    )
    return f"""
WITH RECURSIVE `n`(`i`) AS (SELECT 1 UNION ALL SELECT `i` + 1 FROM `n` WHERE `i` < :rows)
INSERT INTO `profile`
SELECT randomblob(32), 1, 0, 18 + abs(random()) % 50, `w`.`country`,
  20000 + abs(random()) % 180000, 20000 + abs(random()) % 280000,
  abs(random()) % 10 < 3, abs(random()) % 2, {enum_exprs},
  coalesce(`w`.`place`, 'place-' || (1001 + abs(random()) % 9000)),
  1700000000 + abs(random()) % 2592000, random()
FROM `n` CROSS JOIN `place_weight` AS `w` ON `w`.`lo` = (SELECT max(`lo`) FROM `place_weight`
  WHERE `lo` <= (random() & 9007199254740991) / 9007199254740992.0 + 0 * `n`.`i`)
"""  # `+ 0 * n.i` makes the subquery correlated, so random() is drawn once per row; ENUMS order
# equals the profile column order after `wants_more_kids`


ENUM_PARAMS = {c: "".join(code * n for code, n in v) for c, v in ENUMS.items()}

# 3 to 5 interests per profile (3 always, a 4th half of the time, a 5th a quarter); OR IGNORE
# drops the rare duplicate draw out of 200 values
INTEREST_SQL = """
INSERT OR IGNORE INTO `interest`
SELECT `p`.`key`, 'i' || (abs(random()) % 200) FROM `profile` AS `p` CROSS JOIN
  (SELECT 1 AS `k` UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5) AS `t`
WHERE `t`.`k` <= 3 OR (`t`.`k` = 4 AND abs(random()) % 2 = 0) OR (`t`.`k` = 5 AND abs(random()) % 4 = 0)
"""


def build(path, rows):
    for ext in ("", "-journal"):
        if os.path.exists(path + ext):
            os.remove(path + ext)
    db = sqlite3.connect(path, isolation_level=None)
    db.executescript("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; PRAGMA cache_size = -800000;")
    db.executescript(SCHEMA)
    db.execute("BEGIN")
    db.executemany("INSERT INTO `place_weight` VALUES (?, ?, ?, ?)", place_weights())
    t = {}
    for name, run in (
        ("fill_profile", lambda: db.execute(fill_sql(), {"rows": rows, **ENUM_PARAMS})),
        ("fill_interest", lambda: db.execute(INTEREST_SQL)),
    ):
        s = time.monotonic()
        run()
        t[name] = time.monotonic() - s
        say(f"  {name}: {t[name]:.1f}s")
        guard()
    db.execute("COMMIT")
    for ddl in INDEXES:
        s = time.monotonic()
        db.execute(ddl)
        t[ddl.split("`")[1]] = time.monotonic() - s
        say(f"  {ddl.split('`')[1]}: {t[ddl.split('`')[1]]:.1f}s")
        guard()
    s = time.monotonic()
    db.execute("ANALYZE")
    t["analyze"] = time.monotonic() - s
    say(f"  analyze: {t['analyze']:.1f}s")
    db.close()
    return t


# ---- queries ----------------------------------------------------------------------------------
class Q:
    """One query class on one place rank: the SQL text and how to draw its bound values."""

    def __init__(self, cls, rank, country, rng):
        self.cls, self.rank, self.rng = cls, rank, rng
        if cls == "place_in":
            places = [f"place-{k}" for k in range(1, 9)]
            self.scope = "`country` = ? AND `place` IN (" + ", ".join("?" * 8) + ")"
            self.sp = [country, *places]
        elif cls == "country_rare":
            self.scope, self.sp = "`country` = ?", [country]
        else:
            self.scope, self.sp = "`country` = ? AND `place` = ?", [country, f"place-{rank}"]
        # constant identifiers only; values are bound
        parts = {
            "place_only": [],
            "typical": ["`orientation` = ?", "`age` BETWEEN ? AND ?"],
        }
        parts["place_in"] = parts["typical"]
        parts["rare"] = parts["typical"] + ["`education` = ?", "`has_kids` = ?"]
        parts["country_rare"] = parts["rare"] + ["`fitness_level` = ?"]
        self.filt = " AND ".join(parts[cls]) or "1"
        self.cols = {
            "place_only": "`r`",
            "typical": "`r`, `orientation`, `age`",
            "place_in": "`r`, `orientation`, `age`",
            "rare": "`r`, `orientation`, `age`, `education`, `has_kids`",
            "country_rare": "`r`, `orientation`, `age`, `education`, `has_kids`, `fitness_level`",
        }[cls]
        self.sql = (
            f"SELECT `key`, `r` FROM (SELECT `key`, {self.cols} FROM `profile` WHERE {self.scope} "
            f"AND `r` BETWEEN ? AND ? ORDER BY `r` LIMIT ?) WHERE {self.filt} LIMIT ?"
        )
        self.count_sql = (
            f"SELECT count(*) FROM (SELECT 1 FROM `profile` WHERE {self.scope} "
            f"AND `r` BETWEEN ? AND ? ORDER BY `r` LIMIT ?)"
        )

    def fp(self):
        """Filter values: typical = orientation 50 % x age +-5 (22 %); rare adds education 30 % x
        has_kids 30 %; country_rare narrows age to +-2 and adds fitness_level 50 %."""
        if self.cls == "place_only":
            return []
        half = 2 if self.cls == "country_rare" else 5
        a = self.rng.randint(18 + half, 67 - half)
        f = ["str", a - half, a + half]
        if self.cls in ("rare", "country_rare"):
            f += ["ed1", 1]
        if self.cls == "country_rare":
            f += ["fl1"]
        return f

    def plan(self, db):
        args = self.sp + [0, MAXR, CAP] + self.fp() + [PAGE]
        return "\n".join(r[3] for r in db.execute("EXPLAIN QUERY PLAN " + self.sql, args))

    def run(self, db, stats):
        """One search: returns (seconds, found, scanned or None, partial). Counting is untimed."""
        s, fp = self.rng.randint(MINR, MAXR), self.fp()
        found, scanned, spent, known = 0, 0, 0.0, True
        for lo, hi in ((s, MAXR), (MINR, max(MINR, s - 1))):
            budget, need = CAP - scanned, PAGE - found
            if need <= 0 or budget <= 0:
                break
            t = time.perf_counter()
            rows = db.execute(self.sql, self.sp + [lo, hi, budget] + fp + [need]).fetchall()
            spent += time.perf_counter() - t
            found += len(rows)
            if len(rows) < need:  # range exhausted or budget spent: count what was read
                scanned += db.execute(self.count_sql, self.sp + [lo, hi, budget]).fetchone()[0]
            elif stats:
                scanned += db.execute(
                    f"SELECT count(*) FROM `profile` WHERE {self.scope} AND `r` BETWEEN ? AND ?",
                    self.sp + [lo, rows[-1][1]],
                ).fetchone()[0]
            else:
                known = False
        return spent, found, (scanned if known else None), found < PAGE and scanned >= CAP


def pct(xs, p):
    return statistics.quantiles(xs, n=100, method="inclusive")[p - 1] if len(xs) > 1 else xs[0]


def measure_cell(db, q, n):
    times, scans, partial, found = [], [], 0, 0
    for i in range(n):
        guard()
        t, f, sc, p = q.run(db, stats=i % 5 == 0)
        times.append(t * 1000)
        found += f
        partial += p
        if sc is not None:
            scans.append(sc)
    return {
        "queries": n,
        "p50_ms": statistics.median(times),
        "p99_ms": pct(times, 99),
        "rows_scanned": statistics.mean(scans) if scans else None,
        "found_mean": found / n,
        "partial": partial / n,
        "plan": q.plan(db),
        "temp_btree": "TEMP B-TREE" in q.plan(db),
    }


def qps(path, country, seconds=5, threads=2):
    done, stop = [0] * threads, time.monotonic() + seconds

    def work(k):
        db = sqlite3.connect(path)
        q = Q("typical", 1, country, random.Random(k))
        while time.monotonic() < stop:
            q.run(db, stats=False)
            done[k] += 1

    ts = [threading.Thread(target=work, args=(k,)) for k in range(threads)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    return sum(done) / seconds


CELLS = [  # class, ranks, queries per cell
    ("place_only", [1, 100, 10_000], 1000),
    ("typical", [1, 100, 10_000], 1000),
    ("place_in", [None], 1000),
    ("rare", [1, 100, 10_000], 200),
    ("country_rare", [None], 200),
]


def measure(path, rows, run):
    db = sqlite3.connect(path)
    db.execute("PRAGMA cache_size = -65536")
    rng, country = random.Random(42), COUNTRIES[0]
    run["classes"] = {}
    for cls, ranks, n in CELLS:
        for rank in ranks:
            s = time.monotonic()
            cell = measure_cell(db, Q(cls, rank, country, rng), n)
            run["classes"].setdefault(cls, {})[str(rank or "-")] = cell
            say(f"  {cls} rank {rank}: p50 {cell['p50_ms']:.2f} ms p99 {cell['p99_ms']:.2f} ms "
                f"scanned {cell['rows_scanned']} partial {cell['partial']:.2f} ({time.monotonic() - s:.1f}s)")
    db.close()
    guard()
    run["qps_typical_2threads"] = qps(path, country)
    say(f"  qps typical on 2 threads: {run['qps_typical_2threads']:.0f}")


def extrapolate(out):
    runs = out["runs"]
    done = sorted(int(n) for n, r in runs.items() if "classes" in r and "qps_typical_2threads" in r)
    if not done:
        return
    big = str(done[-1])
    out["bytes_per_row"] = runs[big]["db_bytes"] / done[-1]
    out["qps"] = runs[big]["qps_typical_2threads"]
    out["error_2_5m"], out["extrapolation_ok"] = None, False
    if len(done) >= 3:
        t = lambda n: runs[str(n)]["classes"]["typical"]["1"]["p50_ms"]
        (n1, n2), n3 = done[:2], done[-1]
        b = (t(n2) - t(n1)) / (math.log2(n2) - math.log2(n1))
        a = t(n1) - b * math.log2(n1)
        pred = a + b * math.log2(n3)
        out["error_2_5m"] = abs(pred - t(n3)) / t(n3)
        out["extrapolation_ok"] = out["error_2_5m"] <= 0.30
        t10 = max(a + b * math.log2(10_000_000), t(n3))  # latency never falls with N
        out["qps"] = out["qps"] * t(n3) / t10
        out["fit_typical_p50_ms"] = {"a": a, "b_per_log2": b, "pred_at_largest": pred, "p50_at_10m": t10}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rows", type=int, action="append", required=True)
    ap.add_argument("--dir", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    os.makedirs(a.dir, exist_ok=True)
    need, free = max(a.rows) * 400 * 1.5, shutil.disk_usage(a.dir).free
    if need > 10 * 2**30 or need > free:
        sys.exit(f"refusing to start: expected {need / 2**30:.1f} GiB, limit 10 GiB, free {free / 2**30:.1f} GiB")
    out = {"sqlite": sqlite3.sqlite_version, "runs": {}, "partial_run": False}
    try:
        for rows in sorted(a.rows):
            say(f"== {rows} rows")
            path = os.path.join(a.dir, f"profiles-idx-{rows}.db")
            run = out["runs"][str(rows)] = {}
            try:
                run["build_s"] = build(path, rows)
                run["db_bytes"] = os.path.getsize(path)
                run["bytes_per_row"] = run["db_bytes"] / rows
                say(f"  file {run['db_bytes'] / 2**20:.0f} MiB, {run['bytes_per_row']:.0f} B/row")
                measure(path, rows, run)
                run["rss_kb"] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            finally:
                for ext in ("", "-journal"):
                    if os.path.exists(path + ext):
                        os.remove(path + ext)
    except OutOfTime:
        out["partial_run"] = True
        say("10-minute budget exhausted: writing a partial report")
    extrapolate(out)
    with open(a.out, "w") as f:
        json.dump(out, f, indent=1)
    say(f"wrote {a.out}")


if __name__ == "__main__":
    main()
