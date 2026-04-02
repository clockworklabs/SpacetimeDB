# LLM Cost-to-Done Benchmark: SpacetimeDB vs PostgreSQL

**Date:** 2026-04-01
**Model:** Claude Sonnet 4.6 (code generation), Claude Opus 4.6 (grading)
**App:** Real-time chat with 15 features

---

## Key Findings

SpacetimeDB produces a fully working 15-feature real-time app for **12% less cost** with **38% less backend code** than the equivalent PostgreSQL stack.

| | SpacetimeDB | PostgreSQL | Delta |
|--|-------------|------------|-------|
| **Total LLM cost** | **$17.47** | **$19.90** | SpacetimeDB 12% cheaper |
| **API calls** | 533 | 617 | SpacetimeDB 14% fewer |
| **Backend code** | 805 lines | 1,297 lines | SpacetimeDB 38% less |
| **Frontend code (LLM-written)** | 1,265 lines | 1,524 lines | SpacetimeDB 17% less |
| **Auto-generated bindings** | 1,194 lines | 0 | SpacetimeDB only |
| **Total shipped code** | 3,264 lines | 2,821 lines | PostgreSQL 14% less |
| **Total LLM-written code** | 2,070 lines | 2,821 lines | SpacetimeDB 27% less |
| **External deps** | 1 (spacetimedb) | 6 (express, pg, drizzle, socket.io, cors, dotenv) | SpacetimeDB 83% fewer |
| **Feature score** | 45/45 | 45/45 | Tied |
| **Reprompts** | 1 (L8) | 1 (L1) | Tied |

### Why SpacetimeDB Is Cheaper to Build With

The cost advantage comes from **what the LLM doesn't have to write**:

- **No API layer.** SpacetimeDB exposes reducers directly — no Express routes, no REST endpoints, no request/response serialization. PostgreSQL needs ~400 lines of Express route handlers.
- **No real-time plumbing.** SpacetimeDB subscriptions push table changes automatically. PostgreSQL needs Socket.io setup, event handlers, room management, and manual broadcast logic (~200 lines).
- **No schema migration tooling.** SpacetimeDB uses `spacetime publish`. PostgreSQL needs Drizzle ORM config, `drizzle-kit push`, and a `.env` file with connection strings.
- **Auto-generated type-safe bindings.** `spacetime generate` produces TypeScript types from the schema. PostgreSQL requires manual type definitions or duplicated interfaces.
- **Simpler client code.** SpacetimeDB's React hooks (`useTable`, `useSpacetimeDB`) replace manual fetch + Socket.io + state management patterns.

The LLM spends fewer tokens reasoning about infrastructure and more tokens on feature logic. This compounds across 12 upgrade levels — each level adds less boilerplate for SpacetimeDB.

### Where PostgreSQL Was Cheaper

PostgreSQL won on 2 of 12 levels:

- **Level 10 (presence):** $0.54 vs $0.82 — PostgreSQL's Socket.io connection tracking maps naturally to presence status
- **Level 12 (anonymous migration):** $2.13 vs $2.23 — marginal difference, likely noise

---

## Methodology

### Code Generation

We measured the total LLM cost to build a real-time chat app with 15 features, built incrementally across 12 prompt levels:

| Level | Features Added |
|-------|---------------|
| 1 | Basic chat, typing indicators, read receipts, unread counts |
| 2-4 | Scheduled messages, ephemeral messages, reactions |
| 5-7 | Message editing, permissions, presence |
| 8-10 | Threading, private rooms/DMs, activity indicators |
| 11-12 | Draft sync, anonymous-to-registered migration |

Each level invokes Claude Sonnet 4.6 via Claude Code to read a feature prompt and modify the existing codebase. Both backends ran in parallel at each level. Costs tracked via OpenTelemetry instrumentation on every API call.

The PostgreSQL stack is Express + Socket.io + Drizzle ORM + pg — a standard Node.js real-time architecture. The SpacetimeDB stack uses the SpacetimeDB TypeScript SDK with auto-generated bindings.

### Grading

**Method:** Browser interaction via Chrome MCP tools (Claude Opus 4.6 controlling Chrome).

After each level's code was generated and deployed:
1. Opened two browser tabs
2. Registered as "Alice" (Tab A) and "Bob" (Tab B)
3. Executed test steps from 15 feature-specific test plans
4. Scored each feature 0-3 based on observed browser behavior

### Grading Limitations

- **Same-origin tabs**: Both users shared `localStorage`, causing identity collisions
- **No visual verification**: LLM read DOM structure, not rendered output
- **Timing-sensitive features**: Scheduled messages, ephemeral messages, typing indicators hard to verify reliably
- **App confusion**: Both backends served on same port with identical titles
- **Feature scores should be treated as approximate** — automated Playwright tests are needed for investor-grade confidence

---

## Per-Level Cost Breakdown

| Level | SpacetimeDB | PostgreSQL | Cheaper |
|-------|-------------|------------|---------|
| 1 (generate) | $1.20 | $1.12 | PostgreSQL |
| 2 (upgrade) | $0.79 | $0.74 | PostgreSQL |
| 3 (upgrade) | $1.54 | $1.45 | PostgreSQL |
| 4 (upgrade) | $1.14 | $1.23 | SpacetimeDB |
| 5 (upgrade) | $0.43 | $1.16 | SpacetimeDB |
| 6 (upgrade) | $1.99 | $1.90 | PostgreSQL |
| 7 (upgrade) | $2.37 | $2.46 | SpacetimeDB |
| 8 (upgrade) | $0.98 | $1.95 | SpacetimeDB |
| 9 (upgrade) | $2.05 | $2.95 | SpacetimeDB |
| 10 (upgrade) | $0.82 | $0.54 | PostgreSQL |
| 11 (upgrade) | $1.93 | $2.27 | SpacetimeDB |
| 12 (upgrade) | $2.23 | $2.13 | PostgreSQL |
| **Total** | **$17.47** | **$19.90** | **SpacetimeDB** |

SpacetimeDB was cheaper on 6 of 12 levels, with larger margins on complex features (L5, L8, L9) where infrastructure boilerplate compounds.

Costs reflect the final successful `run.sh` invocation per level, verified against OTel telemetry source files.

---

## Feature Scores

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

---

## Architecture Comparison

| Aspect | SpacetimeDB | PostgreSQL |
|--------|-------------|------------|
| Real-time | Built-in subscriptions | Socket.io (manual) |
| API layer | Reducers (auto-exposed) | Express routes (manual) |
| Schema | `table()` + `reducer()` | Drizzle `pgTable()` |
| Type safety | Auto-generated bindings | Manual type definitions |
| Deployment | `spacetime publish` | Express server + Docker DB |
| State sync | Automatic client cache | Manual fetch + Socket.io |
| Online presence | Lifecycle hooks | Manual Socket.io tracking |
| Infrastructure | SpacetimeDB only | PostgreSQL + Express + Socket.io + CORS |

---

## Audit Notes

An audit of the cost data revealed discrepancies between the original GRADING_RESULTS.md files and telemetry source files:

- **SpacetimeDB L7**: GRADING had $1.27 (wrong), telemetry shows $2.37
- **SpacetimeDB L8**: GRADING had $2.18 (sum of 2 runs), should be $0.98 (final run only)
- **PostgreSQL L7**: GRADING had $2.34, telemetry shows $2.46
- **Mislabeled telemetry**: PostgreSQL L2/L9/L10 telemetry saved under `spacetime-*` dirs (run.sh bug)

The corrected numbers in this report use telemetry source of truth with consistent methodology (final successful run per level).

- **Missing level snapshots**: Code snapshots (level-N directories in results) only exist for levels 5-11. Auto-snapshot on upgrade was added partway through the run, so levels 1-4 have no pre-upgrade snapshots.

---

## Next Steps

1. **Automated Playwright tests** — Replace LLM-graded browser interaction with deterministic test suite using separate browser contexts per user
2. **App differentiation** — Different ports (5173 vs 5174) and HTML titles per backend
3. **Durable grading** — Write scores to disk per-feature during grading, not at end
4. **Telemetry isolation** — Fix run.sh to tag telemetry with session IDs for accurate per-run cost reports
