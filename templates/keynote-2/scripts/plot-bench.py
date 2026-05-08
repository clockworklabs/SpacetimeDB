#!/usr/bin/env python3
"""Plot TPS and latency-percentile over time for a single alpha across connectors.

Reads timeSeries arrays from runs/test-1-*.json (added by core/runner.ts)
and emits a stacked TPS + latency chart per alpha.

Usage:
  python3 plot-bench.py [alpha] [outfile] [--runs-dir DIR] [--exclude conn1,conn2] [--latency p50|p95|p99]

Examples:
  python3 plot-bench.py 0
  python3 plot-bench.py 1.5
  python3 plot-bench.py 1.5 contended.png
  python3 plot-bench.py 1.5 no-stdb.png --exclude spacetimedb
  python3 plot-bench.py 1.5 chart.png --runs-dir D:/keynote-2-runs --latency p95
"""
import argparse
import json
import sys
from pathlib import Path
import matplotlib.pyplot as plt

DEFAULT_RUNS_DIR = Path.home() / "SpacetimeDB/templates/keynote-2/runs"


def load_run(path):
    data = json.loads(path.read_text())
    r = data["results"][0]
    return {
        "path": path.name,
        "connector": r["file"].replace(".ts", ""),
        "alpha": data["alpha"],
        "ts": r["res"].get("timeSeries", []),
    }


def plot(runs, alpha, outfile, exclude=None, latency="p99"):
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(11, 8), sharex=True)

    matched = [r for r in runs if r["alpha"] == alpha and r["ts"]]
    if exclude:
        matched = [r for r in matched if r["connector"] not in exclude]

    if not matched:
        print(f"no runs with timeSeries data found at alpha={alpha}", file=sys.stderr)
        sys.exit(1)

    latency_key = f"{latency}_ms"

    # one line per run; group by connector for legend de-dup
    seen_connectors = {}
    for r in matched:
        ts = r["ts"]
        x = [p["tSec"] for p in ts]

        label = r["connector"]
        if label in seen_connectors:
            label = None
        else:
            seen_connectors[r["connector"]] = True

        ax1.plot(x, [p["tps"] for p in ts], label=label, linewidth=2, alpha=0.85)
        ax2.plot(x, [p[latency_key] for p in ts], label=label, linewidth=2, alpha=0.85)

    contention = "uncontended" if alpha == 0 else f"alpha={alpha}"
    title = f"alpha={alpha}  ({contention})"
    if exclude:
        title += f"  (excluded: {','.join(exclude)})"

    ax1.set_ylabel("TPS")
    ax1.set_title(title)
    ax1.legend(loc="upper right")
    ax1.grid(True, alpha=0.3)

    ax2.set_ylabel(f"{latency} latency (ms)")
    ax2.set_xlabel("Time (s)")
    ax2.set_yscale("log")
    ax2.legend(loc="upper left")
    ax2.grid(True, alpha=0.3)

    plt.tight_layout()
    plt.savefig(outfile, dpi=120)
    print(f"wrote {outfile} ({len(matched)} runs, latency={latency})")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("alpha", nargs="?", type=float, default=0)
    parser.add_argument("outfile", nargs="?", default=None)
    parser.add_argument("--runs-dir", type=Path, default=DEFAULT_RUNS_DIR,
                        help="directory containing test-1-*.json files")
    parser.add_argument("--exclude", default="",
                        help="comma-separated connectors to skip")
    parser.add_argument("--latency", choices=["p50", "p95", "p99"], default="p99",
                        help="which latency percentile to plot in the bottom panel")
    args = parser.parse_args()

    outfile = args.outfile or f"bench-alpha{args.alpha}-{args.latency}.png"
    exclude = [c.strip() for c in args.exclude.split(",") if c.strip()]

    runs = [load_run(p) for p in sorted(args.runs_dir.glob("test-1-*.json"))]
    plot(runs, args.alpha, outfile, exclude=exclude, latency=args.latency)
