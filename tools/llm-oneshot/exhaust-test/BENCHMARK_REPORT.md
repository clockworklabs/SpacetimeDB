# LLM Cost-to-Done Benchmark: SpacetimeDB vs PostgreSQL

**Date:** 2026-04-01
**Model:** Claude Sonnet 4.6 (code generation), Claude Opus 4.6 (grading)
**App:** Real-time chat with 15 features

---

## What We Did

We measured the total LLM cost to build a fully working real-time chat app using two different backends: **SpacetimeDB** and **PostgreSQL** (Express + Socket.io + Drizzle ORM).

Both backends implemented the same 15 features, built incrementally across 12 prompt levels:

| Level | Features Added |
|-------|---------------|
| 1 | Basic chat, typing indicators, read receipts, unread counts |
| 2-4 | Scheduled messages, ephemeral messages, reactions |
| 5-7 | Message editing, permissions, presence |
| 8-10 | Threading, private rooms/DMs, activity indicators |
| 11-12 | Draft sync, anonymous-to-registered migration |

Each level was generated via `run.sh --upgrade`, which invokes Claude Sonnet 4.6 in Claude Code to read the feature prompt and modify the existing codebase. Both backends ran in parallel at each level. Costs were tracked automatically via OpenTelemetry instrumentation on every API call.

---

## How We Graded

**Method:** Manual browser interaction via Chrome MCP tools (Claude Opus 4.6 controlling Chrome).

After each level's code was generated and deployed, we:
1. Opened two browser tabs at `http://localhost:5173`
2. Registered as "Alice" (Tab A) and "Bob" (Tab B)
3. Executed test steps from 15 feature-specific test plans
4. Scored each feature 0-3 based on observed browser behavior

### Grading Limitations (Known Issues)

- **Same-origin tabs**: Both users shared `localStorage`, causing identity collisions between test runs
- **No multi-user isolation**: Both tabs ran in the same browser context, not separate profiles
- **Accessibility tree grading**: The LLM read DOM structure, not visual output — some visual bugs may have been missed
- **Timing-sensitive features** (scheduled messages, ephemeral messages, typing indicators) were difficult to verify reliably
- **Self-grading risk**: The grading LLM could not independently verify features — it relied on its own DOM observations
- **App confusion**: Both backends served on the same port (5173) and had identical HTML titles ("SpacetimeDB Chat"), making it unclear which app was being tested at times
- **Cost data discrepancies**: Some per-level costs in GRADING_RESULTS.md don't match telemetry source files (see Audit section)

**Bottom line:** Feature scores should be treated as approximate. The grading process needs to be replaced with automated Playwright tests before these results are used for any external communication.

---

## Results

### Feature Scores

Both backends scored **45/45** (all 15 features passing at 3/3).

| # | Feature | SpacetimeDB | PostgreSQL |
|---|---------|-------------|------------|
| 1 | Basic Chat | 3/3 | 3/3 |
| 2 | Typing Indicators | 3/3 | 3/3 |
| 3 | Read Receipts | 3/3 | 3/3 |
| 4 | Unread Counts | 3/3 | 3/3 |
| 5 | Scheduled Messages | 3/3 | 3/3 |
| 6 | Ephemeral Messages | 3/3 | 3/3 |
| 7 | Message Reactions | 3/3 | 3/3 |
| 8 | Message Editing | 3/3 | 3/3 |
| 9 | Real-Time Permissions | 3/3 | 3/3 |
| 10 | Rich User Presence | 3/3 | 3/3 |
| 11 | Message Threading | 3/3 | 3/3 |
| 12 | Private Rooms & DMs | 3/3 | 3/3 |
| 13 | Room Activity Indicators | 3/3 | 3/3 |
| 14 | Draft Sync | 3/3 | 3/3 |
| 15 | Anonymous Migration | 3/3 | 3/3 |

### Cost Comparison

| Metric | SpacetimeDB | PostgreSQL |
|--------|-------------|------------|
| **Total Cost** | $17.47 | $19.90 |
| **API Calls** | 533 | 617 |
| **Backend LOC** | 805 | 1,297 |
| **Frontend LOC** | 1,265 | 1,524 |
| **Reprompts** | 0 | 1 (L1) |

*Costs reflect the final successful run per level. Some levels had multiple `run.sh` invocations due to infrastructure issues — only the last successful run is counted.*

### Per-Level Cost Breakdown

| Level | SpacetimeDB | PostgreSQL |
|-------|-------------|------------|
| 1 (generate) | $1.20 | $1.12 |
| 2 (upgrade) | $0.79 | $0.74 |
| 3 (upgrade) | $1.54 | $1.45 |
| 4 (upgrade) | $1.14 | $1.23 |
| 5 (upgrade) | $0.43 | $1.16 |
| 6 (upgrade) | $1.99 | $1.90 |
| 7 (upgrade) | $2.37 | $2.46 |
| 8 (upgrade) | $0.98 | $1.95 |
| 9 (upgrade) | $2.05 | $2.95 |
| 10 (upgrade) | $0.82 | $0.54 |
| 11 (upgrade) | $1.93 | $2.27 |
| 12 (upgrade) | $2.23 | $2.13 |
| **Total** | **$17.47** | **$19.90** |

---

## Audit Notes

An audit of the cost data revealed discrepancies between GRADING_RESULTS.md files and telemetry source files:

- **SpacetimeDB L7**: GRADING had $1.27 (wrong), telemetry shows $2.37
- **SpacetimeDB L8**: GRADING had $2.18 (sum of 2 runs), should be $0.98 (final run only)
- **PostgreSQL L7**: GRADING had $2.34, telemetry shows $2.46
- **Mislabeled telemetry**: PostgreSQL L2/L9/L10 telemetry directories were saved under `spacetime-*` names (run.sh bug — both backends launch in parallel)
- **Multiple run.sh invocations**: L8 had 2 runs (spacetime) and 3 runs (postgres), suggesting infrastructure instability at that level

The corrected numbers in this report use the telemetry source of truth with consistent methodology (final successful run per level).

---

## What Needs to Change

1. **Automated test suite** — Replace Chrome MCP grading with Playwright tests using separate browser contexts per user
2. **data-testid attributes** — Add to feature prompt specs so tests work across different generated UIs
3. **App differentiation** — Different HTML titles per backend ("SpacetimeDB Chat" vs "PostgreSQL Chat")
4. **Telemetry directory naming** — Fix run.sh to correctly label backend in telemetry output dirs
5. **Cost report consistency** — Always use final successful run, document methodology
6. **Independent grading** — Grading session should not be the same LLM that generated the code
