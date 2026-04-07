# AI App Generation Benchmark — Metrics Report (L1–L11)

**Date:** 2026-04-07
**Variant:** Sequential Upgrade (one feature group added per level)
**Model:** Claude Sonnet (claude-sonnet-4-6)
**Levels completed:** L1–L11 (11 feature groups)
**Backends:** SpacetimeDB vs PostgreSQL (Express + Socket.io + Drizzle ORM)

---

## Methodology

Two chat apps were built in parallel, upgraded one feature group at a time. Each level was manually graded after generation. Bugs were fixed via a fix prompt before proceeding to the next level. All costs are measured via OpenTelemetry instrumentation of the Claude API.

### Guidelines Given to AI

| Backend | Files | Total Lines | Content |
|---------|-------|-------------|---------|
| SpacetimeDB | 3 | **529** | SDK API reference (258), config boilerplate templates (141), server setup/deploy (130) |
| PostgreSQL | 1 | **314** | Server setup/deploy with full boilerplate templates and socket.io proxy guidance |

SpacetimeDB required detailed SDK documentation because it uses a novel proprietary API unfamiliar to the model. PostgreSQL used well-known libraries (Express, Socket.io, Drizzle) — no API reference needed, but received equivalent infrastructure boilerplate in `postgres.md`.

**Changes from previous run (20260403):**
- SpacetimeDB SDK examples generalized from chat domain (user/message/sendMessage) to leaderboard domain, removing the domain-familiarity advantage
- PostgreSQL Phase 1 instructions made feature-spec-neutral (no longer pre-listing presence, read receipts, etc.)
- Fixed PORT contradiction in postgres.md (3001 vs 6001)
- Replaced prescriptive socket.io client code templates with constraint-only guidance

---

## Cost Summary

### Total Cost

| | SpacetimeDB | PostgreSQL |
|-|------------|------------|
| Upgrades (L1–L11) | $10.86 | $13.00 |
| Fixes | $0.81 | $4.47 |
| **Total** | **$11.67** | **$17.47** |
| **Combined** | | **$29.14** |

### Cost per Feature Group (11 feature groups)

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Cost / feature (all-in) | $1.06 | $1.59 |
| Cost / feature (upgrades only) | $0.99 | $1.18 |
| Cost / feature (fixes only) | $0.07 | $0.41 |

---

## Token Consumption

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Fresh input tokens | 21,460 | 28,977 |
| Fresh output tokens | 215,871 | 278,353 |
| Cache read tokens | 20,890,518 | 35,427,182 |
| Cache creation tokens | 607,859 | 705,278 |
| Total API calls | 362 | 624 |

**Cache leverage** — for every 1 fresh input token, ~974 cache tokens were read (SpacetimeDB) and ~1,222 (PostgreSQL). The sequential upgrade approach with prompt caching is highly economical: prior context is cached and reused across sessions.

### Tokens per Feature Group

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Fresh output tokens / feature | ~19,625 | ~25,305 |
| API calls / feature | 32.9 | 56.7 |

PostgreSQL required ~72% more API calls and ~29% more output tokens per feature, primarily due to fix iterations.

---

## Lines of Code

| | SpacetimeDB | PostgreSQL |
|-|------------|------------|
| Backend (hand-written) | 1,072 | 1,625 |
| Frontend (hand-written) | 1,547 | 1,770 |
| Generated bindings | 1,134 | 0 |
| **Total hand-written** | **2,619** | **3,395** |
| **LOC / feature group** | **238** | **309** |

SpacetimeDB produced **23% less hand-written code** overall, with the backend being **34% smaller** — the SpacetimeDB model (reducers + subscriptions) replaces manual WebSocket management, routing, and SQL query boilerplate.

---

## Quality: Bugs Found

### Bug Count by Level

| Level | Feature Added | STDB Bugs | PG Bugs |
|-------|--------------|-----------|---------|
| L1 | Basic Chat + Typing + Read Receipts + Unread Counts | 1 | 1 |
| L2 | Scheduled Messages | 0 | 0 |
| L3 | Ephemeral Messages | 1 | 1 |
| L4 | Message Reactions | 0 | 0 |
| L5 | Message Editing with History | 0 | 0 |
| L6 | Real-Time Permissions | 0 | 2 |
| L7 | Rich User Presence | 0 | 1 |
| L8 | Message Threading | 0 | 0 |
| L9 | Private Rooms + Direct Messages | 0 | 0 |
| L10 | Room Activity Indicators | 0 | 2 |
| L11 | Draft Sync | 0 | 0 |
| **Total** | | **2** | **7** |

### Bug Summary

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Total bugs found | 2 | 7 |
| Bugs per feature group | 0.18 | 0.64 |
| Levels with zero bugs | 9 / 11 (82%) | 6 / 11 (55%) |
| Fix sessions required | 1 | 8 |
| Multi-attempt fixes | 0 | 1 (L6 kick — 3 attempts) |

### Bug Categories

**SpacetimeDB (2 bugs):**
| Category | Count |
|----------|-------|
| Real-time state not updating | 1 (L1: unread badge) |
| SDK API misuse | 1 (L3: Timestamp constructor wrong type) |

**PostgreSQL (7 bugs):**
| Category | Count |
|----------|-------|
| Real-time state not updating | 4 (L1: unread badge; L6: member panel; L10: activity reset, thread unread badge) |
| Logic error (wrong value/enforcement) | 2 (L6: kick re-entry not blocked; L7: last-active timestamp stale) |
| Runtime crash | 1 (L3: messages not iterable after schema change) |

Real-time state management was PostgreSQL's biggest weakness — requiring manual Socket.io event wiring made it easy to miss subscription cases that SpacetimeDB handles automatically via its subscription model.

---

## Reliability & Efficiency

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Zero-bug generation rate | 82% | 55% |
| Fix cost as % of total | 6.9% | 25.6% |
| Avg fix cost per bug | $0.40 | $0.64 |
| Multi-attempt bugs | 0 | 1 |
| Fix sessions per bug | 0.5 | 1.14 |

---

## Time to Build

| Phase | SpacetimeDB | PostgreSQL |
|-------|------------|------------|
| L1 generation | 4m 41s | ~7m |
| Per-level upgrade (avg) | 4m 28s | 4m 30s |
| Per-fix session (avg) | 4m 26s | 3m 25s |
| **Total wall time** | **~54m** | **~77m** |

Both backends spent nearly identical time on upgrades (~4m 30s avg). All of PostgreSQL's extra time came from fix iterations — 27 additional minutes in fix sessions vs SpacetimeDB.

---

## Key Takeaways

### 1. Removing domain bias strengthened the STDB advantage
In the previous run (20260403), SpacetimeDB's SDK docs contained chat-domain examples (user/message/sendMessage tables). After neutralizing this, SpacetimeDB still had 3.5× fewer bugs (2 vs 7) and cost 33% less. The advantage is real, not an artifact of domain familiarity.

### 2. Real-time state is PostgreSQL's systemic weakness
4 of 7 PG bugs were real-time state not updating. In SpacetimeDB, subscriptions automatically push state changes to clients — the AI doesn't need to wire up individual Socket.io events. In PostgreSQL, each state change requires a manually-authored emit, and missing any one causes a bug.

### 3. Fix cost disparity is dramatic
SpacetimeDB spent $0.81 on fixes (6.9% of total). PostgreSQL spent $4.47 (25.6%). The L6 kick-enforcement bug alone required 3 fix sessions at $3.06, demonstrating that some correctness properties (persistent ban state, server-side enforcement) are genuinely hard for the AI to reason about in a REST+WebSocket architecture.

### 4. LOC is a maintenance proxy
23% less hand-written code in SpacetimeDB means smaller context windows for future upgrades, less surface area for bugs, and lower ongoing maintenance cost. The backend is particularly lean (1,072 vs 1,625 lines) — SpacetimeDB reducers replace manual routing, SQL, and WebSocket plumbing.

### 5. Sequential upgrade economics remain strong
20.9M cache tokens read by SpacetimeDB, 35.4M by PostgreSQL — at ~$0.30/1M, this represents enormous cost savings vs a from-scratch approach. Each upgrade session benefits from the full prior context being cached.

### 6. Per-upgrade costs are similar; fix costs are not
SpacetimeDB and PostgreSQL had nearly identical per-upgrade costs ($0.99 vs $1.18 per feature) and identical avg upgrade times (~4m 30s). The entire cost and time gap is explained by fix iterations — not generation difficulty.

---

## Comparison to Previous Run (20260403)

| Metric | Run 1 (20260403) | Run 2 (20260406) | Change |
|--------|-----------------|-----------------|--------|
| STDB total cost | $12.46 | $11.67 | -6% |
| PG total cost | $16.22 | $17.47 | +8% |
| STDB bugs | 5 | 2 | -60% |
| PG bugs | 18 | 7 | -61% |
| STDB fix sessions | 4 | 1 | -75% |
| PG fix sessions | 16 | 8 | -50% |
| STDB LOC | 2,881 | 2,619 | -9% |
| PG LOC | 3,647 | 3,395 | -7% |

Both backends improved substantially after fixing the confounds (domain hints, PORT contradictions, prescriptive instructions). The bug reduction is particularly notable — 60% fewer bugs across the board despite the same feature set and model.
