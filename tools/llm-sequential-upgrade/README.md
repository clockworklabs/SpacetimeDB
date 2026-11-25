# LLM Sequential Upgrade Benchmark

Automated benchmark harness for measuring AI app-generation cost, bug rate, and code size across backends. Designed to produce directly comparable data for the same app built on different stacks.

Results viewer: https://spacetimedb.com/llms-benchmark-sequential-upgrade

Generated test data (app source, telemetry, cost summaries): https://github.com/clockworklabs/spacetimedb-llm-sequential-upgrade-results

## What this measures

For each backend under test, the harness drives a headless Claude Code session to:

1. Generate a chat app from the L1 feature spec
2. Upgrade through L2-L12 one feature group at a time
3. After each level, a human grades the app against the feature spec
4. Bugs are filed as `BUG_REPORT.md` and fixed via a separate Claude Code session
5. All API costs are captured via OpenTelemetry and written to per-session cost summaries

Side-by-side results give a direct comparison of AI-generation cost across backends for the same functional target.

## Directory contents

The tooling is the Rust crate in [`tools/llm-apps`](../llm-apps), run as `cargo llm-apps sequential <command>` from anywhere in the repository:

- `generate`, `upgrade`, `fix`: headless Claude Code sessions that build, upgrade and fix an app. `upgrade` takes `--composed-prompt` and `--resume-session`.
- `grade`: a Claude Code session that grades the deployed app in Chrome
- `loop` / `benchmark`: drive one backend through every level, or several in parallel
- `reset` / `cleanup`: give an app an empty database; remove an app's dependencies and build output
- `parse-telemetry` / `report`: turn the OpenTelemetry log into per-session cost summaries, and those into a run's BENCHMARK_REPORT.md
- `publish`: push a graded run to the results repo

This directory holds what those commands read:

- `templates/`: canonical `BUG_REPORT.md` / `ITERATION_LOG.md` formats for grading
- `benchmark-viewer.html`: local viewer for METRICS_DATA.json files (open in browser, drop JSON)
- `docker-compose.otel.yaml` / `otel-collector-config.yaml`: OTel collector + PostgreSQL + MongoDB
- `backends/`: per-backend setup / SDK reference documents given to the AI
- `perf-benchmark/`: runtime throughput benchmark (msgs/sec) for the AI-generated apps
- `CLAUDE.md` / `DEVELOP.md` / `GRADING.md` / `GRADING_WORKFLOW.md`: process documentation

## Running a benchmark

```bash
# Prereqs: Claude CLI installed, Docker running, SpacetimeDB installed
docker compose -f docker-compose.otel.yaml up -d

# Generate L1 from scratch
cargo llm-apps sequential generate --backend spacetime --level 1
cargo llm-apps sequential generate --backend postgres --level 1

# Grade, then fix the bugs grading found
cargo llm-apps sequential grade <app-dir>
cargo llm-apps sequential fix <app-dir> --level N

# Upgrade through levels
cargo llm-apps sequential upgrade <app-dir> --level 2 --composed-prompt
# ... continue through L12

# Write the run's report, then publish it
cargo llm-apps sequential report sequential-upgrade/sequential-upgrade-<date>
cargo llm-apps sequential publish sequential-upgrade/sequential-upgrade-<date>
```

Generated apps and telemetry land in `sequential-upgrade/sequential-upgrade-<date>/` locally, which is gitignored. `publish` copies a graded run to the [sequential-upgrade results repo](https://github.com/clockworklabs/spacetimedb-llm-sequential-upgrade-results), leaving out dependencies, build output and telemetry that contains personal data. It expects a checkout of that repository next to your SpacetimeDB checkout and clones one if missing; pass `--results-repo <path>` or set `LLM_SEQUENTIAL_UPGRADE_RESULTS_REPO` to use another.

## Performance benchmark

`perf-benchmark/` contains a runtime stress tool that fires concurrent writers against a generated app's `send_message` handler to measure sustained throughput in messages/sec. See `perf-benchmark/README.md` for usage.
