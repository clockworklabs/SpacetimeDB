# Bench scripts

Helpers for running the keynote benchmark on a single host.

| Script | What it does |
| --- | --- |
| `start-bench.sh` | Bring up every backend service (sqlite-rpc, postgres-rpc, bun-rpc, cockroach + cockroach-rpc, supabase-rpc, convex local) in its own tmux window inside session `bench`. |
| `stop-bench.sh` | Kill every foreground bench process and the tmux session. Leaves Postgres (systemd) and Supabase (Docker) running. |
| `check-bench.sh` | Health-check each service with a single HTTP call. |
| `run-all-benches.sh` | Run the bench across connectors and alphas, capturing per-run TPS/latency/verify output to `/tmp/bench-results.tsv`. |
| `plot-bench.py` | Read the `timeSeries` field from `runs/test-1-*.json` and produce per-alpha TPS + p99-latency charts. Requires matplotlib. |

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

# seed (one-time, or after wiping a DB)
pnpm run prep

# benchmark — args: RUNS SECONDS CONNECTORS_CSV ALPHAS_CSV
scripts/run-all-benches.sh 3 60       # 3 runs x 60s, all connectors, both alphas
scripts/run-all-benches.sh 5 60 sqlite_rpc,postgres_rpc 1.5

# plot
python3 scripts/plot-bench.py 0       # writes bench-alpha0.0.png
python3 scripts/plot-bench.py 1.5

# tear down
scripts/stop-bench.sh
```

## Notes

- `run-all-benches.sh` overwrites `/tmp/bench-results.tsv` on each invocation. Archive it first if you want to preserve a prior sweep.
- All scripts assume the repo lives at `~/SpacetimeDB`. Edit the hardcoded paths if your checkout is elsewhere.
- `plot-bench.py` requires the `timeSeries` field added to `core/runner.ts`. Older `runs/*.json` files without that field are silently skipped.
