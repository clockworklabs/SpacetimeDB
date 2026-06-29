# LLM Sequential Upgrade Benchmark

Automated benchmark harness for measuring AI app-generation cost, bug rate, and code size across backends. Designed to produce directly comparable data for the same app built on different stacks.

Results viewer: https://spacetimedb.com/llms-benchmark-sequential-upgrade

Generated test data (app source, telemetry, cost summaries): https://github.com/clockworklabs/spacetimedb-ai-test-results

## What this measures

For each backend under test, the harness drives a headless Claude Code session to:

1. Generate a chat app from the L1 feature spec
2. Upgrade through L2-L12 one feature group at a time
3. After each level, a human grades the app against the feature spec
4. Bugs are filed as `BUG_REPORT.md` and fixed via a separate Claude Code session
5. All API costs are captured via OpenTelemetry and written to per-session cost summaries

Side-by-side results give a direct comparison of AI-generation cost across backends for the same functional target.

## Directory contents

- `run.sh`: orchestrates generation, upgrade, and fix sessions. Supports `--upgrade`, `--fix`, `--composed-prompt`, `--resume-session`.
- `grade.sh` / `grade-agents.sh` / `grade-playwright.sh`: grading harnesses (manual + automated)
- `benchmark.sh` / `run-loop.sh`: batch runners for parallel or sequential benchmark execution
- `cleanup.sh` / `reset-app.sh`: dev utilities
- `benchmark-viewer.html`: local viewer for METRICS_DATA.json files (open in browser, drop JSON)
- `generate-report.mjs`: aggregate per-session cost-summary.json into a markdown report
- `parse-telemetry.mjs`: parse OTel log stream into per-session cost-summary.json
- `parse-playwright-results.mjs`: convert Playwright JSON output to grading markdown
- `docker-compose.otel.yaml` / `otel-collector-config.yaml`: OTel collector + PostgreSQL
- `backends/`: per-backend setup / SDK reference documents given to the AI
- `perf-benchmark/`: runtime throughput benchmark (msgs/sec) for the AI-generated apps
- `CLAUDE.md` / `DEVELOP.md` / `GRADING.md` / `GRADING_WORKFLOW.md`: process documentation

## Running a benchmark

```bash
# Prereqs: Claude CLI installed, Docker running, SpacetimeDB installed
docker compose -f docker-compose.otel.yaml up -d

# Generate L1 from scratch
./run.sh --backend spacetime --level 1
./run.sh --backend postgres --level 1

# Upgrade through levels
./run.sh --upgrade <app-dir> --level 2 --composed-prompt
# ... continue through L12

# Fix bugs found during grading
./run.sh --fix <app-dir> --level N
```

Generated apps and telemetry land in `sequential-upgrade/sequential-upgrade-<timestamp>/` locally. For published test data from canonical runs, see the [AI Test Results repo](https://github.com/clockworklabs/spacetimedb-ai-test-results).

## Performance benchmark

`perf-benchmark/` contains a runtime stress tool that fires concurrent writers against a generated app's `send_message` handler to measure sustained throughput in messages/sec. See `perf-benchmark/README.md` for usage.
