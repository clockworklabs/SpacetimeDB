#!/usr/bin/env python3
"""Compute detailed stats from each test-1-*.json run in a runs-dir.

Pulls out:
  - aggregate metrics (already in res.*)
  - steady-state window stats (t >= --warmup-sec, default 30)
  - tail window stats (last --tail-sec seconds, default 30)
  - time-series shape: tps min/max/mean/median/stdev, stability (CV)
  - collapse detection: first second where TPS drops below 10% of peak
  - death detection: first second where TPS=0 and stays 0 for 30s

Usage:
  python3 bench-stats.py [--runs-dir DIR] [--warmup-sec N] [--tail-sec N] [--out FILE]

Examples:
  python3 bench-stats.py --runs-dir D:/keynote-2-runs
  python3 bench-stats.py --runs-dir D:/keynote-2-runs --warmup-sec 60 --out stats.tsv
"""
import argparse
import json
import math
import statistics
import sys
from pathlib import Path

DEFAULT_RUNS_DIR = Path.home() / "SpacetimeDB/templates/keynote-2/runs"


def load_run(path):
    data = json.loads(path.read_text())
    r = data["results"][0]
    return {
        "path": path.name,
        "system": r.get("system", r.get("file", "?").replace(".ts", "")),
        "alpha": data["alpha"],
        "seconds": data["seconds"],
        "concurrency": data["concurrency"],
        "agg": r["res"],
        "ts": r["res"].get("timeSeries", []),
    }


def safe_div(a, b):
    return a / b if b else 0.0


def window_stats(ts, t_min, t_max=None):
    """Return aggregate stats for time-series points where t_min <= tSec < t_max."""
    pts = [p for p in ts if p["tSec"] >= t_min and (t_max is None or p["tSec"] < t_max)]
    if not pts:
        return None

    samples = sum(p["samples"] for p in pts)
    duration = sum(1 for _ in pts)  # one point per second
    tps_values = [p["tps"] for p in pts]
    nonzero_tps = [v for v in tps_values if v > 0]
    p50_values = [p["p50_ms"] for p in pts if p["samples"] > 0]
    p99_values = [p["p99_ms"] for p in pts if p["samples"] > 0]

    return {
        "samples": samples,
        "duration_s": duration,
        "tps_mean": safe_div(samples, duration),
        "tps_min": min(tps_values) if tps_values else 0,
        "tps_max": max(tps_values) if tps_values else 0,
        "tps_median": statistics.median(tps_values) if tps_values else 0,
        "tps_stdev": statistics.stdev(tps_values) if len(tps_values) > 1 else 0,
        "tps_cv_pct": (
            (statistics.stdev(tps_values) / statistics.mean(tps_values) * 100)
            if len(tps_values) > 1 and statistics.mean(tps_values) > 0
            else 0
        ),
        "zero_seconds": sum(1 for v in tps_values if v == 0),
        "p50_ms_median": statistics.median(p50_values) if p50_values else 0,
        "p99_ms_median": statistics.median(p99_values) if p99_values else 0,
        "p99_ms_max": max(p99_values) if p99_values else 0,
    }


def find_collapse(ts, threshold_pct=10):
    """First tSec where TPS drops below threshold_pct% of peak.
    Returns None if it never collapses."""
    if not ts:
        return None
    peak = max(p["tps"] for p in ts)
    if peak == 0:
        return None
    threshold = peak * threshold_pct / 100
    # require sustained drop (3 consecutive seconds below threshold)
    streak = 0
    for p in ts:
        if p["tps"] < threshold:
            streak += 1
            if streak >= 3:
                return p["tSec"] - 2  # first second of the streak
        else:
            streak = 0
    return None


def find_death(ts, hold_sec=30):
    """First tSec where TPS hits 0 and stays 0 for hold_sec seconds.
    Returns None if never dies."""
    streak = 0
    for p in ts:
        if p["tps"] == 0:
            streak += 1
            if streak >= hold_sec:
                return p["tSec"] - hold_sec + 1
        else:
            streak = 0
    return None


def fmt(v):
    if v is None:
        return ""
    if isinstance(v, float):
        if math.isnan(v) or math.isinf(v):
            return ""
        if abs(v) >= 100:
            return f"{v:.1f}"
        if abs(v) >= 1:
            return f"{v:.2f}"
        return f"{v:.4f}"
    return str(v)


COLUMNS = [
    "system",
    "alpha",
    "duration_s",
    "concurrency",
    # aggregate (whole run)
    "agg_tps",
    "agg_samples",
    "agg_p50_ms",
    "agg_p95_ms",
    "agg_p99_ms",
    "agg_collision_rate",
    # steady-state (after warmup)
    "ss_tps_mean",
    "ss_tps_median",
    "ss_tps_stdev",
    "ss_tps_cv_pct",
    "ss_p50_ms",
    "ss_p99_ms",
    "ss_zero_secs",
    # tail (last N seconds)
    "tail_tps_mean",
    "tail_p50_ms",
    "tail_p99_ms",
    # time-series shape
    "ts_tps_min",
    "ts_tps_max",
    "ts_p99_max",
    # collapse / death
    "collapse_at_s",
    "death_at_s",
    # source
    "file",
]


def row_for_run(run, warmup_sec, tail_sec):
    agg = run["agg"]
    ts = run["ts"]
    duration = run["seconds"]

    ss = window_stats(ts, warmup_sec) if ts else None
    tail_start = duration - tail_sec
    tail = window_stats(ts, tail_start) if ts else None
    full = window_stats(ts, 0) if ts else None

    return {
        "system": run["system"],
        "alpha": run["alpha"],
        "duration_s": duration,
        "concurrency": run["concurrency"],
        "agg_tps": agg.get("tps"),
        "agg_samples": agg.get("samples"),
        "agg_p50_ms": agg.get("p50_ms"),
        "agg_p95_ms": agg.get("p95_ms"),
        "agg_p99_ms": agg.get("p99_ms"),
        "agg_collision_rate": agg.get("collision_rate"),
        "ss_tps_mean": ss["tps_mean"] if ss else None,
        "ss_tps_median": ss["tps_median"] if ss else None,
        "ss_tps_stdev": ss["tps_stdev"] if ss else None,
        "ss_tps_cv_pct": ss["tps_cv_pct"] if ss else None,
        "ss_p50_ms": ss["p50_ms_median"] if ss else None,
        "ss_p99_ms": ss["p99_ms_median"] if ss else None,
        "ss_zero_secs": ss["zero_seconds"] if ss else None,
        "tail_tps_mean": tail["tps_mean"] if tail else None,
        "tail_p50_ms": tail["p50_ms_median"] if tail else None,
        "tail_p99_ms": tail["p99_ms_median"] if tail else None,
        "ts_tps_min": full["tps_min"] if full else None,
        "ts_tps_max": full["tps_max"] if full else None,
        "ts_p99_max": full["p99_ms_max"] if full else None,
        "collapse_at_s": find_collapse(ts),
        "death_at_s": find_death(ts),
        "file": run["path"],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runs-dir", type=Path, default=DEFAULT_RUNS_DIR)
    parser.add_argument("--warmup-sec", type=int, default=30,
                        help="Seconds to skip before computing steady-state stats")
    parser.add_argument("--tail-sec", type=int, default=30,
                        help="Tail window size for last-N-seconds stats")
    parser.add_argument("--out", type=Path, default=None,
                        help="Write TSV to this file in addition to stdout")
    args = parser.parse_args()

    files = sorted(args.runs_dir.glob("test-1-*.json"))
    if not files:
        print(f"no test-1-*.json files in {args.runs_dir}", file=sys.stderr)
        sys.exit(1)

    rows = [row_for_run(load_run(p), args.warmup_sec, args.tail_sec) for p in files]

    # sort by (alpha, system) for readability
    rows.sort(key=lambda r: (r["alpha"], r["system"]))

    header = "\t".join(COLUMNS)
    body = "\n".join("\t".join(fmt(r[c]) for c in COLUMNS) for r in rows)
    output = header + "\n" + body + "\n"

    print(output)
    if args.out:
        args.out.write_text(output)
        print(f"\nwrote {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
