# AI App Generation Benchmark — Metrics Report (L1–L11)

**Date:** 2026-04-04
**Variant:** Sequential Upgrade (one feature group added per level)
**Model:** Claude Sonnet (claude-sonnet-4-6)
**Levels completed:** L1–L11 (13 feature groups, 10+ features per feature group)
**Backends:** SpacetimeDB vs PostgreSQL (Express + Socket.io + Drizzle ORM)

---

## Methodology

Two chat apps were built in parallel, upgraded one feature group at a time. Each level was manually graded after generation. Bugs were fixed via a fix prompt before proceeding. All costs are measured via OpenTelemetry instrumentation of the Claude API.

### Guidelines Given to AI

| Backend | Files | Total Lines | Content |
|---------|-------|-------------|---------|
| SpacetimeDB | 4 | **574** | SDK API reference (258), config boilerplate templates (141), server setup/deploy (130), language/stack setup (45) |
| PostgreSQL | 2 | **357** | Server setup/deploy with full boilerplate templates (313), language/stack setup (44) |

SpacetimeDB required detailed SDK documentation because it uses a novel proprietary API unfamiliar to the model. PostgreSQL used well-known libraries (Express, Socket.io, Drizzle) — no API reference needed, but received equivalent infrastructure boilerplate in `postgres.md`.

**Known confounds (discovered post-run):**

*SpacetimeDB:* The SDK reference contained chat-domain examples throughout: a `user` table with `online: t.bool()` presence field, subscribe calls referencing `user` and `message` tables, a reducer named `sendMessage`, and a complete example that was a minimal chat app (`user` + `message` tables). These directly overlap with the app being built.

*PostgreSQL:* The `postgres.md` Phase 1 instructions explicitly named chat-domain entities and features: `REST endpoints for rooms, messages, users` and `Socket.io events for real-time: typing, messages, presence, read receipts`. This pre-listed feature categories (presence, read receipts) that don't appear until L6–L7 in the spec.

Both confounds have been corrected for future runs (SpacetimeDB examples generalized to leaderboard domain; PostgreSQL Phase 1 instructions made feature-spec-neutral). The net effect on this run is uncertain. SpacetimeDB's domain hints were more deeply embedded (in API docs used every session); PostgreSQL's were a one-time architectural hint at generation time. Neither is likely to explain the large L6–L11 quality gap, where features diverge far from any example.

---

## Cost Summary

### Total Cost

| | SpacetimeDB | PostgreSQL |
|-|------------|------------|
| Upgrades (L1–L11) | $10.99 | $7.50 |
| Fixes | $1.47 | $8.72 |
| **Total** | **$12.46** | **$16.22** |
| **Combined** | | **$28.68** |

### Cost per Feature Group (13 feature groups through L11)

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Cost / feature (all-in) | $0.96 | $1.25 |
| Cost / feature (upgrades only) | $0.85 | $0.58 |
| Cost / feature (fixes only) | $0.11 | $0.67 |

---

## Token Consumption

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Fresh input tokens | 352 | 605 |
| Fresh output tokens | 179,621 | 260,728 |
| Cache read tokens | 28,336,554 | 31,731,184 |
| Cache creation tokens | 491,496 | 743,243 |
| Total API calls | 291 | 462 |

**Cache leverage is enormous** — for every 1 fresh input token, ~80,000 cache tokens were read. This is what makes the sequential upgrade approach economical: prior context is cached and reused across sessions.

### Tokens per Feature Group

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Fresh output tokens / feature | ~13,817 | ~20,056 |
| API calls / feature | 22.4 | 35.5 |

PostgreSQL requires ~45% more API calls and ~45% more output tokens per feature, primarily due to fix iterations.

---

## Lines of Code

| | SpacetimeDB | PostgreSQL |
|-|------------|------------|
| Backend (hand-written) | 659 | 1,120 |
| Frontend (hand-written) | 2,222 | 2,527 |
| Generated bindings | 556 | 0 |
| **Total hand-written** | **2,881** | **3,647** |
| **LOC / feature group** | **221** | **281** |

SpacetimeDB produces **21% less hand-written code** overall, with the backend being **41% smaller** — the SpacetimeDB model (reducers + subscriptions) replaces manual WebSocket management, routing, and SQL query boilerplate.

---

## Quality: Bugs Found

### Bug Count by Level

| Level | Feature Added | STDB Bugs | PG Bugs |
|-------|--------------|-----------|---------|
| L1 | Basic Chat + Typing + Read Receipts + Unread | 1 | 3 |
| L2 | Scheduled Messages | 0 | 0 |
| L3 | Ephemeral Messages | 0 | 0 |
| L4 | Message Reactions | 0 | 0 |
| L5 | Message Editing with History | 2 | 1 |
| L6 | Real-Time Permissions | 1 | 2 |
| L7 | Rich User Presence | 0 | 3 |
| L8 | Message Threading | 0 | 2 |
| L9 | Private Rooms + DMs | 0 | 7 |
| L10 | Room Activity Indicators | 0 | 0 |
| L11 | Draft Sync | 1 | 0 |
| **Total** | | **5** | **18** |

*L9 count includes 3 DM bugs found during L10 grading (no DM button, DM name offline, DM room disappears offline) — attributed to L9 since all are L9 features.*

### Bug Summary

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Total bugs found | 5 | 18 |
| Bugs per feature group | 0.38 | 1.38 |
| Levels with zero bugs | 7 / 11 (64%) | 5 / 11 (45%) |
| Fix sessions required | 4 | 16 |
| First-attempt fix success | 4 / 4 (100%) | 16 / 18 (89%) |
| Multi-attempt fixes | 0 | 2 |

### Bug Categories (PostgreSQL)

| Category | Count |
|----------|-------|
| Real-time state not updating | 5 |
| Missing UI element | 5 |
| Data not persisted server-side | 3 |
| Logic error (wrong value/calculation) | 2 |
| Race condition / stale reference | 3 |

Real-time state management was PostgreSQL's biggest weakness — requiring manual WebSocket event handling made it easy for the AI to miss subscription cases that SpacetimeDB handles automatically.

---

## Reliability & Efficiency

| Metric | SpacetimeDB | PostgreSQL |
|--------|------------|------------|
| Zero-bug generation rate | 64% | 45% |
| First-attempt fix success rate | 100% | 89% |
| Fix cost as % of total | 11.8% | 53.8% |
| Avg fix cost per bug | $0.29 | $0.48 |
| Time in fixes vs upgrades | 15% / 85% | 54% / 46% |
| Multi-attempt bugs | 0 | 2 |

---

## Time to Build

| Phase | SpacetimeDB | PostgreSQL |
|-------|------------|------------|
| L1 generation | 6m 6s | 5m 0s |
| Per-level upgrade (avg) | 4m 26s | 2m 41s |
| Per-fix session (avg) | 1m 21s | 4m 57s |
| **Total wall time** | **~48m** | **~77m** |

SpacetimeDB takes longer per upgrade (more complex backend changes each level), but dramatically less time in fixes. PostgreSQL upgrades are faster but fix sessions dominate the timeline.

---

## Key Takeaways for AI App Generation

### 1. Backend choice drives fix costs more than generation costs
SpacetimeDB's declarative model (reducers + auto-subscriptions) produces significantly fewer real-time bugs. The AI can focus on *what* the app does rather than *how* to plumb WebSocket events.

### 2. SDK documentation has outsized ROI — but watch for domain bias
SpacetimeDB received 574 lines of guidelines vs PostgreSQL's 357. Despite more documentation overhead, SpacetimeDB had fewer bugs. Good in-context documentation guides the AI away from incorrect API usage — but the SpacetimeDB SDK reference contained chat-domain examples (user/message/sendMessage) that may have given L1 a head start. Future runs use a generalized leaderboard example to isolate the SDK-quality advantage from the domain-familiarity advantage.

### 3. First-attempt fix reliability is a stronger signal than bug count
SpacetimeDB: 100% first-attempt fix success. PostgreSQL: 89% (2 of 18 bugs required two fix sessions each). When a fix fails on the first try, it signals the AI is struggling to reason about the bug — which compounds cost and time.

### 4. LOC is a proxy for maintenance cost
21% less hand-written code in SpacetimeDB means less surface area for bugs, smaller context windows for future upgrades, and lower ongoing maintenance burden.

### 5. Feature complexity is not uniform
L9 (Private Rooms + DMs) was the most expensive feature to get right: $4.53 in fix costs for PostgreSQL across 7 distinct bugs spanning private room visibility, invite flows, DM creation, and DM offline state handling. Features involving complex access control, persistent membership state, and multi-user UX edge cases consistently trip up AI generation more than UI-heavy features.

### 6. Cache leveraging is the key to economic sequential builds
28–31M cache tokens read across both backends at ~$0.30/1M = ~$9 in cache reads vs what would have been ~$140+ if every token were fresh. Sequential upgrade with prompt caching is highly cost-effective for iterative app development.

---

## Projection to Full Feature Set (L19)

Based on observed cost curves (approximately linear per upgrade, fix costs trending upward with complexity):

| Metric | SpacetimeDB (est.) | PostgreSQL (est.) |
|--------|-------------------|-------------------|
| Total cost to L19 | ~$22–26 | ~$30–38 |
| Total bugs to L19 | ~8–10 | ~26–32 |
| Total time to L19 | ~80–90m | ~120–150m |

*Estimates. Actual results depend on complexity of L12–L19 features.*
