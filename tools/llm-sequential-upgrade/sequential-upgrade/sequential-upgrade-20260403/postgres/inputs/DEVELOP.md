# Exhaust Test — Developer Guide

How to set up, run, and interpret the LLM cost-to-done benchmark.

---

## What This Does

Measures the **total token cost to reach a fully working chat app** by alternating between two agents:

1. **Code Agent** (headless, `run.sh`) — generates code, fixes bugs, deploys. Token-tracked via OpenTelemetry.
2. **Grade Agent** (interactive Claude Code) — tests in Chrome via MCP, writes bug reports. NOT token-tracked.

Only the Code Agent's tokens count toward the benchmark. Grading cost is the same for both SpacetimeDB and PostgreSQL, so it's excluded.

### The Loop

```
run.sh --level 1          → Code Agent generates & deploys app (tokens tracked)
  ↓
You (in Claude Code)      → Grade Agent tests in Chrome, writes BUG_REPORT.md
  ↓
run.sh --fix <app-dir>    → Code Agent reads bugs, fixes code, redeploys (tokens tracked)
  ↓
You (in Claude Code)      → Grade Agent retests, writes updated BUG_REPORT.md or GRADING_RESULTS.md
  ↓
... repeat until all features pass or iteration limit hit
```

---

## Prerequisites

### 1. SpacetimeDB

```bash
spacetime start
```

### 2. Docker (for OpenTelemetry Collector)

```bash
cd tools/llm-oneshot/llm-sequential-upgrade
docker compose -f docker-compose.otel.yaml up -d
```

### 3. Claude Code CLI

Needs `claude` on PATH, or `npx @anthropic-ai/claude-code` works as fallback.

### 4. Chrome + Claude MCP Extension

Required for the grading agent (interactive session). Chrome must be open with the "Claude in Chrome" MCP extension active.

### 5. Node.js

Required for SpacetimeDB TypeScript backend, Vite dev server, and `parse-telemetry.mjs`.

---

## Running a Benchmark

### Step 1: Generate & Deploy (headless, token-tracked)

```bash
cd tools/llm-oneshot/llm-sequential-upgrade
./run.sh --level 1 --backend spacetime
```

This:
1. Runs pre-flight checks (SpacetimeDB, Docker, OTel, prompts)
2. Launches headless Claude Code with OTel telemetry enabled
3. Generates backend + client code, builds, deploys (SpacetimeDB: localhost:5173, PostgreSQL: localhost:5174)
4. Parses telemetry → `COST_REPORT.md`
5. Prints the app directory path

### Step 2: Grade (interactive, not token-tracked)

In this Claude Code session (or a new interactive one), say:

```
Grade the app at sequential-upgrade/sequential-upgrade-YYYYMMDD/results/spacetime/chat-app-<timestamp>
```

Or use the helper script:
```bash
./grade.sh sequential-upgrade/sequential-upgrade-YYYYMMDD/results/spacetime/chat-app-<timestamp>
```

The grading agent will:
1. Open Chrome, navigate to the backend's port (5173 for SpacetimeDB, 5174 for PostgreSQL)
2. Test each feature using the test plans
3. Score features 0-3
4. If bugs found: write `BUG_REPORT.md` in the app directory
5. Write/update `ITERATION_LOG.md` and `GRADING_RESULTS.md`

### Step 3: Fix (headless, token-tracked)

If bugs were found:

```bash
./run.sh --fix sequential-upgrade/sequential-upgrade-YYYYMMDD/results/spacetime/chat-app-<timestamp>
```

This:
1. Reads `BUG_REPORT.md` from the app directory
2. Fixes the code, republishes if needed
3. Tokens tracked via OTel (cumulative with Step 1)

### Step 4: Re-grade

Back in Claude Code:
```
Re-grade the app at sequential-upgrade/sequential-upgrade-YYYYMMDD/results/spacetime/chat-app-<timestamp>
```

Repeat Steps 3-4 until all features pass.

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--level` | `1` | Prompt level (1-12). Level 1 = 4 features, Level 12 = all 15 |
| `--backend` | `spacetime` | `spacetime` or `postgres` |
| `--variant` | `sequential-upgrade` | Test variant: `sequential-upgrade` or `one-shot` |
| `--fix <dir>` | — | Fix mode: read BUG_REPORT.md, fix code, redeploy |
| `--upgrade <dir>` | — | Upgrade mode: add features to existing app |
| `--resume-session` | — | Resume prior Claude session for cache reuse |

### Recommended Test Levels

| Level | Features | Est. Duration | Good For |
|-------|----------|---------------|----------|
| 1 | 4 (basic chat, typing, receipts, unread) | 5-15 min | Pipeline validation |
| 5 | 8 (+ scheduled, ephemeral, reactions, edit) | 15-30 min | Mid-complexity |
| 12 | All 15 features | 30-60+ min | Full benchmark |

---

## Output Files

### Per-run directory structure
```
llm-sequential-upgrade/<variant>/<variant>-YYYYMMDD/
  BENCHMARK_REPORT.md     # Comparison report (written manually after all grading)
  inputs/                 # Frozen snapshot of all inputs used for this run
  results/
    <backend>/chat-app-<timestamp>/
      GRADING_RESULTS.md  # Per-feature scores (written by grade agent)
      ITERATION_LOG.md    # Per-iteration progress log (both agents append)
      BUG_REPORT.md       # Current bugs for fix agent to read (deleted when all pass)
      backend/            # Generated SpacetimeDB or PostgreSQL backend
      client/             # Generated React client
  telemetry/
    <backend>-level<N>-<timestamp>/
      metadata.json       # Run parameters, timing, session ID
      COST_REPORT.md      # Exact token counts per API call
```

### Shared telemetry (OTel Collector output)
```
llm-sequential-upgrade/telemetry/
  logs.jsonl              # Raw OTLP log records (shared across all runs)
  metrics.jsonl           # Raw OTLP metrics
```

---

## Understanding the Results

### GRADING_RESULTS.md

- **Feature scores**: 0-3 per feature, scored from observed browser behavior
- **Reprompt log**: Every bug fix iteration with category and description
- **Reprompt efficiency**: 0-10 scale (0 reprompts = 10, 16+ reprompts = 0)

### COST_REPORT.md

- **Total tokens**: Exact input + output token counts across all Code Agent API calls
- **Cache read tokens**: Tokens served from prompt cache (reduced cost)
- **Cost (USD)**: Total dollar cost of the code generation + fix iterations
- **Per-call breakdown**: Every API call with model, tokens, cost, duration

### Key Comparison Metrics

| Metric | What It Shows |
|--------|---------------|
| Total tokens to done | Raw LLM efficiency — fewer = easier to build with |
| Iterations to done | Fix cycles needed — fewer = less debugging |
| Final feature score | Quality of the final app |
| Lines of code | Code complexity — smaller = simpler for LLMs |
| External dependencies | Infrastructure complexity |

---

## Troubleshooting

### OTel Collector not receiving data

```bash
docker compose -f docker-compose.otel.yaml logs
ls -la telemetry/logs.jsonl
```

### SpacetimeDB publish fails

```bash
spacetime server ping local
spacetime start  # if not running
```

### Chrome MCP tools not working (grading session)

- Chrome must be open before starting the grading session
- "Claude in Chrome" extension must be installed and active
- Only works in interactive Claude Code sessions (not `--print` mode)

### Session runs out of context

- Try a lower level first
- The ITERATION_LOG.md preserves progress even if a session dies

---

## Running a Full Comparison

### Sequential Upgrade (default)

```bash
# Generate level 1, then upgrade through each level
./run.sh --level 1 --backend spacetime
# (grade, fix loop...)
./run.sh --upgrade <app-dir> --level 2
# ... continue through level 12

# Same for PostgreSQL
./run.sh --level 1 --backend postgres
# (grade, fix loop...)
./run.sh --upgrade <app-dir> --level 2
# ... continue through level 12
```

### One-Shot

```bash
# Generate all 15 features in a single prompt
./run.sh --variant one-shot --backend spacetime
./run.sh --variant one-shot --backend postgres
```

---

## File Structure

```
llm-sequential-upgrade/
  CLAUDE.md                        # Instructions for the Code Agent
  DEVELOP.md                       # This file (for humans)
  run.sh                           # Code Agent launcher (generate/fix/upgrade)
  grade.sh                         # Grade Agent launcher (interactive Chrome MCP)
  grade-playwright.sh              # Grade via Playwright (optional, deterministic)
  docker-compose.otel.yaml         # OTel Collector container
  otel-collector-config.yaml       # Collector config (OTLP → JSON files)
  parse-telemetry.mjs              # Telemetry → COST_REPORT.md
  backends/
    spacetime.md                   # SpacetimeDB-specific phases
    spacetime-sdk-rules.md         # SpacetimeDB SDK patterns
    spacetime-templates.md         # Code templates
    postgres.md                    # PostgreSQL-specific phases
  test-plans/
    feature-01-basic-chat.md       # Per-feature browser test scripts
    ...
    feature-15-anonymous-migration.md
    playwright/                    # Optional Playwright test suite
  telemetry/                       # Shared OTel Collector output
  sequential-upgrade/              # Sequential upgrade test variant
    sequential-upgrade-YYYYMMDD/   # Dated run with results, telemetry, inputs
  one-shot/                        # One-shot test variant
    one-shot-YYYYMMDD/
```

---

## Architecture

```
                    TOKEN-TRACKED                      NOT TRACKED
               ┌─────────────────────┐          ┌─────────────────────┐
               │                     │          │                     │
   run.sh ────▶│  Code Agent         │          │  Grade Agent        │◀──── You
               │  (claude --print)   │          │  (interactive CC)   │      (in Claude Code)
               │                     │          │                     │
               │  • Generate code    │          │  • Chrome MCP       │
               │  • Build & deploy   │   Bug    │  • Test features    │
               │  • Fix bugs ◀───────│── Report │  • Score 0-3        │
               │  • Redeploy         │──────────▶  • Write BUG_REPORT │
               │                     │          │  • Write GRADING    │
               └────────┬────────────┘          └─────────────────────┘
                        │
               OTel telemetry
                        │
               ┌────────▼────────────┐
               │  OTel Collector     │
               │  → logs.jsonl       │
               │  → COST_REPORT.md   │
               └─────────────────────┘
```
