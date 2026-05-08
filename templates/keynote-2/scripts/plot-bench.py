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


def plot(runs, alpha, outfile, exclude=None, latency="p99", metric="both"):
    matched = [r for r in runs if r["alpha"] == alpha and r["ts"]]
    if exclude:
        matched = [r for r in matched if r["connector"] not in exclude]

    if not matched:
        print(f"no runs with timeSeries data found at alpha={alpha}", file=sys.stderr)
        sys.exit(1)

    latency_key = f"{latency}_ms"

    if metric == "both":
        fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(11, 8), sharex=True)
        axes = [(ax1, "tps", "TPS"), (ax2, latency_key, f"{latency} latency (ms)")]
    elif metric == "tps":
        fig, ax1 = plt.subplots(1, 1, figsize=(11, 5))
        axes = [(ax1, "tps", "TPS")]
    elif metric == "latency":
        fig, ax1 = plt.subplots(1, 1, figsize=(11, 5))
        axes = [(ax1, latency_key, f"{latency} latency (ms)")]
    else:
        raise ValueError(f"unknown metric: {metric}")

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

        for ax, key, _ in axes:
            ax.plot(x, [p[key] for p in ts], label=label, linewidth=2, alpha=0.85)

    contention = "uncontended" if alpha == 0 else f"alpha={alpha}"
    title = f"alpha={alpha}  ({contention})"
    if exclude:
        title += f"  (excluded: {','.join(exclude)})"

    for i, (ax, key, ylabel) in enumerate(axes):
        ax.set_ylabel(ylabel)
        ax.legend(loc="upper right" if key == "tps" else "upper left")
        ax.grid(True, alpha=0.3)
        if key != "tps":
            ax.set_yscale("log")
        if i == 0:
            ax.set_title(title)

    axes[-1][0].set_xlabel("Time (s)")

    plt.tight_layout()
    plt.savefig(outfile, dpi=120)
    print(f"wrote {outfile} ({len(matched)} runs, metric={metric}, latency={latency})")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("alpha", nargs="?", type=float, default=0)
    parser.add_argument("outfile", nargs="?", default=None)
    parser.add_argument("--runs-dir", type=Path, default=DEFAULT_RUNS_DIR,
                        help="directory containing test-1-*.json files")
    parser.add_argument("--exclude", default="",
                        help="comma-separated connectors to skip")
    parser.add_argument("--latency", choices=["p50", "p95", "p99"], default="p99",
                        help="which latency percentile to plot")
    parser.add_argument("--metric", choices=["both", "tps", "latency"], default="both",
                        help="show TPS only, latency only, or both panels")
    args = parser.parse_args()

    # If outfile is just a filename (not a path), put it in the runs dir.
    if args.outfile:
        outfile_path = Path(args.outfile)
        if outfile_path.parent == Path("."):
            outfile_path = args.runs_dir / outfile_path
    else:
        outfile_path = args.runs_dir / f"bench-alpha{args.alpha}-{args.metric}-{args.latency}.png"

    exclude = [c.strip() for c in args.exclude.split(",") if c.strip()]

    runs = [load_run(p) for p in sorted(args.runs_dir.glob("test-1-*.json"))]
    plot(runs, args.alpha, str(outfile_path), exclude=exclude, latency=args.latency, metric=args.metric)
