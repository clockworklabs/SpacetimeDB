# Bench scripts

Helpers for running the keynote benchmark on a single host.

| Script | What it does |
| --- | --- |
| `start-bench.sh` | Bring up every backend service (sqlite-rpc, postgres-rpc, bun-rpc, cockroach + cockroach-rpc, supabase-rpc, convex local) in its own tmux window inside session `bench`. |
| `stop-bench.sh` | Kill every foreground bench process and the tmux session. Leaves Postgres (systemd) and Supabase (Docker) running. |
| `check-bench.sh` | Health-check each service with a single HTTP call. |
| `bench-stats.py` | Read `runs/test-1-*.json` and emit a TSV with aggregate, steady-state, tail-window, and time-series stats. Detects collapse/death points. |
| `plot-bench.py` | Read the `timeSeries` field from `runs/test-1-*.json` and produce per-alpha TPS + latency-percentile charts. Requires matplotlib. |

Sweep orchestration (multiple alphas, multiple runs, optional state reset) is now built into `pnpm run bench` itself. See the project README's "Test Command" section.

## Typical flow

```bash
# one-time prerequisites
sudo systemctl start postgresql       # Postgres as system service
supabase start                        # Supabase Docker stack
sudo -u postgres psql -c "CREATE DATABASE bun_bench;"

# bring services up
scripts/start-bench.sh
tmux attach -t bench                  # poke around if needed
scripts/check-bench.sh                # confirm all green

# seed once, then sweep alphas and connectors
pnpm run prep
pnpm run bench --alpha 0,1.5 --connectors postgres_rpc,bun --seconds 300

# multi-run sweep with auto-reset between alphas
pnpm run bench --alpha 0,1.5 --connectors postgres_rpc --seconds 300 --runs 3 --prep-between-alphas

# stats + plots from the per-run JSONs
python3 scripts/bench-stats.py --runs-dir runs
python3 scripts/plot-bench.py 0
python3 scripts/plot-bench.py 1.5

# tear down
scripts/stop-bench.sh
```

## Notes

- `bench-stats.py` and `plot-bench.py` glob `runs/test-1-*.json`. To keep separate sweeps from mixing, organize JSONs into subdirectories per sweep and point `--runs-dir` at each one.
- `plot-bench.py` requires the `timeSeries` field on each run, added by the current `core/runner.ts`. Older JSON files without that field are silently skipped.
- All scripts resolve paths relative to the script file, so the checkout can live anywhere.
