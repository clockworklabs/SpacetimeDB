#!/usr/bin/env python3
"""Plot TPS and p99 latency over time for a single alpha across connectors.

Reads timeSeries arrays from runs/test-1-*.json (added by core/runner.ts)
and emits a stacked TPS + p99-latency chart per alpha.

Usage:
  python3 plot-bench.py [alpha] [outfile]

Examples:
  python3 plot-bench.py 0
  python3 plot-bench.py 1.5
  python3 plot-bench.py 1.5 contended.png
"""
import json
import sys
from pathlib import Path
import matplotlib.pyplot as plt

RUNS_DIR = Path.home() / "SpacetimeDB/templates/keynote-2/runs"


def load_run(path):
    data = json.loads(path.read_text())
    r = data["results"][0]
    return {
        "path": path.name,
        "connector": r["file"].replace(".ts", ""),
        "alpha": data["alpha"],
        "ts": r["res"].get("timeSeries", []),
    }


def plot(runs, alpha, outfile):
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(11, 8), sharex=True)

    matched = [r for r in runs if r["alpha"] == alpha and r["ts"]]
    if not matched:
        print(f"no runs with timeSeries data found at alpha={alpha}", file=sys.stderr)
        sys.exit(1)

    # one line per run; group by connector for color reuse
    seen_connectors = {}
    for r in matched:
        ts = r["ts"]
        x = [p["tSec"] for p in ts]

        label = r["connector"]
        if label in seen_connectors:
            label = None  # avoid duplicate legend entries when there are multiple runs
        else:
            seen_connectors[r["connector"]] = True

        ax1.plot(x, [p["tps"] for p in ts], label=label, linewidth=2, alpha=0.85)
        ax2.plot(x, [p["p99_ms"] for p in ts], label=label, linewidth=2, alpha=0.85)

    contention = "uncontended" if alpha == 0 else f"alpha={alpha}"
    ax1.set_ylabel("TPS")
    ax1.set_title(f"alpha={alpha}  ({contention})")
    ax1.legend(loc="upper right")
    ax1.grid(True, alpha=0.3)

    ax2.set_ylabel("p99 latency (ms)")
    ax2.set_xlabel("Time (s)")
    ax2.set_yscale("log")
    ax2.legend(loc="upper left")
    ax2.grid(True, alpha=0.3)

    plt.tight_layout()
    plt.savefig(outfile, dpi=120)
    print(f"wrote {outfile} ({len(matched)} runs)")


if __name__ == "__main__":
    alpha = float(sys.argv[1]) if len(sys.argv) > 1 else 0
    outfile = sys.argv[2] if len(sys.argv) > 2 else f"bench-alpha{alpha}.png"

    runs = [load_run(p) for p in sorted(RUNS_DIR.glob("test-1-*.json"))]
    plot(runs, alpha, outfile)
