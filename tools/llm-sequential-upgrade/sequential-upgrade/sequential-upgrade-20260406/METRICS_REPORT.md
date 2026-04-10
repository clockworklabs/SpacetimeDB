# AI App Generation Benchmark — Metrics Report (L1–L12)

**Date:** 2026-04-07 (L1–L11), 2026-04-10 (L12 added)
**Variant:** Sequential Upgrade (one feature group added per level)
**Model:** Claude Sonnet (claude-sonnet-4-6)
**Levels completed:** L1–L12 (12 feature groups)
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
| Upgrades (L1–L12) | $11.81 | $14.56 |
| Fixes | $0.81 | $5.11 |
| **Total** | **$12.62** | **$19.68** |
| **Combined** | | **$32.30** |

### Cost per Feature Group (12 feature groups)

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Cost / feature (all-in) | $1.05 | $1.64 |
| Cost / feature (upgrades only) | $0.98 | $1.21 |
| Cost / feature (fixes only) | $0.07 | $0.43 |

---

## Token Consumption

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Fresh input tokens | 21,490 | 29,081 |
| Fresh output tokens | 226,079 | 304,198 |
| Cache read tokens | 22,870,337 | 40,443,349 |
| Cache creation tokens | 661,888 | 792,679 |
| Total API calls | 390 | 718 |

**Cache leverage** — for every 1 fresh input token, ~1,064 cache tokens were read (SpacetimeDB) and ~1,391 (PostgreSQL). The sequential upgrade approach with prompt caching is highly economical: prior context is cached and reused across sessions.

### Tokens per Feature Group

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Fresh output tokens / feature | ~18,840 | ~25,350 |
| API calls / feature | 32.5 | 59.8 |

PostgreSQL required ~84% more API calls and ~35% more output tokens per feature, primarily due to fix iterations.

---

## Lines of Code

| | SpacetimeDB | PostgreSQL |
|-|------------|------------|
| Backend (hand-written) | 876 | 1,710 |
| Frontend (hand-written code) | 1,589 | 1,922 |
| Generated bindings | 1,174 | 0 |
| **Total hand-written code** | **2,465** | **3,632** |
| **LOC / feature group** | **205** | **303** |

SpacetimeDB produced **32% less hand-written code** overall, with the backend being **49% smaller** — the SpacetimeDB model (reducers + subscriptions) replaces manual WebSocket management, routing, and SQL query boilerplate.

*Methodology: counts `.ts` and `.tsx` files in the app's `src/` and `server/src/` directories at L12, excluding `node_modules`, `dist`, `level-N` snapshot copies, CSS files, and generated SpacetimeDB bindings (counted separately).*

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
| L12 | Anonymous to Registered Migration | 0 | 1 |
| **Total** | | **2** | **8** |

### Bug Summary

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Total bugs found | 2 | 8 |
| Bugs per feature group | 0.17 | 0.67 |
| Levels with zero bugs | 10 / 12 (83%) | 6 / 12 (50%) |
| Fix sessions required | 1 | 10 |
| Multi-attempt fixes | 0 | 2 (L6 kick — 3 attempts; L12 guest persistence — 2 attempts) |

### Bug Categories

**SpacetimeDB (2 bugs):**
| Category | Count |
|----------|-------|
| Real-time state not updating | 1 (L1: unread badge) |
| SDK API misuse | 1 (L3: Timestamp constructor wrong type) |

**PostgreSQL (8 bugs):**
| Category | Count |
|----------|-------|
| Real-time state not updating | 4 (L1: unread badge; L6: member panel; L10: activity reset, thread unread badge) |
| Data not persisted | 1 (L12: guest session lost on refresh) |
| Logic error (wrong value/enforcement) | 2 (L6: kick re-entry not blocked; L7: last-active timestamp stale) |
| Runtime crash | 1 (L3: messages not iterable after schema change) |

Real-time state management was PostgreSQL's biggest weakness — requiring manual Socket.io event wiring made it easy to miss subscription cases that SpacetimeDB handles automatically via its subscription model. The L12 guest-persistence bug is a related architectural issue: SpacetimeDB's identity token is automatically persisted in `localStorage` by the SDK, so a guest's session survives a page refresh for free. PostgreSQL had to explicitly implement `localStorage` + a rehydration fetch endpoint, and the LLM missed it in both the original upgrade and the first fix attempt (the fix introduced a route-ordering regression that broke `/api/users/online`, requiring a second fix iteration).

---

## Reliability & Efficiency

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Zero-bug generation rate | 83% | 50% |
| Fix cost as % of total | 6.4% | 26.0% |
| Avg fix cost per bug | $0.40 | $0.64 |
| Multi-attempt bugs | 0 | 2 |
| Fix sessions per bug | 0.5 | 1.25 |

---

## Time to Build

| Phase | SpacetimeDB | PostgreSQL |
|-------|------------|------------|
| L1 generation | 4m 41s | ~7m |
| Per-level upgrade (avg) | 4m 21s | 4m 32s |
| Per-fix session (avg) | 4m 26s | 2m 59s |
| **Total wall time** | **~57m** | **~84m** |

Both backends spent nearly identical time on upgrades (~4m 25s avg). All of PostgreSQL's extra time came from fix iterations — 25 additional minutes in fix sessions vs SpacetimeDB.

---

## Key Takeaways

### 1. Removing domain bias strengthened the STDB advantage
In the previous run (20260403), SpacetimeDB's SDK docs contained chat-domain examples (user/message/sendMessage tables). After neutralizing this, SpacetimeDB still had 4× fewer bugs (2 vs 8) and cost 36% less. The advantage is real, not an artifact of domain familiarity.

### 2. Real-time state is PostgreSQL's systemic weakness
4 of 8 PG bugs were real-time state not updating. In SpacetimeDB, subscriptions automatically push state changes to clients — the AI doesn't need to wire up individual Socket.io events. In PostgreSQL, each state change requires a manually-authored emit, and missing any one causes a bug.

### 3. Free session persistence for STDB; manual for PG
The L12 anonymous migration exposed a second architectural advantage: SpacetimeDB's identity token is automatically persisted to `localStorage` by the SDK, so a guest's session survives a page refresh without any code. PostgreSQL had to explicitly implement client-side session storage + a user-lookup endpoint — and got it wrong twice (missed entirely in the upgrade; first fix introduced a route-ordering regression; a second fix iteration was required). Both PG runs (20260403 and 20260406) independently hit the same bug, confirming it's structural, not stochastic.

### 4. Fix cost disparity is dramatic
SpacetimeDB spent $0.81 on fixes (6.4% of total). PostgreSQL spent $5.11 (26.0%). The L6 kick-enforcement bug alone required 3 fix sessions at $3.06, and the L12 guest-persistence bug required 2 fix sessions ($0.65), demonstrating that some correctness properties (persistent ban state, client-side session management) are genuinely hard for the AI to reason about in a REST+WebSocket architecture.

### 5. LOC is a maintenance proxy
32% less hand-written code in SpacetimeDB means smaller context windows for future upgrades, less surface area for bugs, and lower ongoing maintenance cost. The backend is particularly lean (876 vs 1,710 lines at L12 — a 49% reduction) — SpacetimeDB reducers replace manual routing, SQL, and WebSocket plumbing.

### 6. Sequential upgrade economics remain strong
22.9M cache tokens read by SpacetimeDB, 40.4M by PostgreSQL — at ~$0.30/1M, this represents enormous cost savings vs a from-scratch approach. Each upgrade session benefits from the full prior context being cached.

### 7. Per-upgrade costs are similar; fix costs are not
SpacetimeDB and PostgreSQL had nearly identical per-upgrade costs ($0.98 vs $1.21 per feature) and similar avg upgrade times (~4m 25s). The entire cost and time gap is explained by fix iterations — not generation difficulty.

---

## Comparison to Previous Run (20260403)

| Metric | Run 1 (20260403) | Run 2 (20260406) | Change |
|--------|-----------------|-----------------|--------|
| STDB total cost | $13.33 | $12.62 | -5% |
| PG total cost | $17.80 | $19.68 | +11% |
| STDB bugs | 5 | 2 | -60% |
| PG bugs | 19 | 8 | -58% |
| STDB fix sessions | 4 | 1 | -75% |
| PG fix sessions | 17 | 10 | -41% |
| STDB LOC (no CSS) | 2,143 | 2,465 | +15% |
| PG LOC (no CSS) | 2,943 | 3,632 | +23% |

Both backends improved substantially after fixing the confounds (domain hints, PORT contradictions, prescriptive instructions). The bug reduction is particularly notable — ~60% fewer bugs across the board despite the same feature set and model. Both runs agree on the direction of the PG-vs-STDB gap (STDB cheaper, fewer bugs); the refined 20260406 methodology widens it.
