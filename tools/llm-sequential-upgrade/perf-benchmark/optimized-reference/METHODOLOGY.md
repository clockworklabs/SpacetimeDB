# Optimized Reference Versions

These files contain the optimized reference versions of the `sendMessage` handlers.
There are only two comparison points:
- Raw: the AI-generated baseline
- Optimized: the one-pass improved implementation

## What changed (STDB)

Same features as AI-generated. Implementation changes only:
- Membership check: use `userIdentity.filter(ctx.sender)` instead of `roomId.filter(roomId)` + `[...spread]` + `toHexString()` string allocation
- Read receipt update: same fix (identity index, no spread, no string alloc)
- Typing indicator cleanup: same fix
- User existence check: kept
- Room existence check: kept
- Message insert: kept
- All validation: kept

## What changed (PG 20260406)

Same features as AI-generated. Implementation changes only:
- Rate limit: kept
- Banned check: kept
- Membership check: kept
- Message insert: kept (blocking await, need the result)
- Room emit: kept
- lastSeen update: made non-blocking (fire without await)
- Notification fanout query + loop: made non-blocking (fire without await)
- Thread reply counting: kept
- Typing indicator cleanup: kept
- Activity tracking: kept

## What changed (PG 20260403)

Same features as AI-generated. Implementation changes only:
- Message insert: kept (blocking await)
- User lookup for username: made non-blocking (fire without await, emit after lookup resolves)
- Room activity broadcast: kept
- Response sent immediately after insert instead of after user lookup

## What changed (MongoDB 20260616)

Same features as AI-generated. Implementation changes only (produced by a clean
`claude-sonnet-4-6` first-principles pass — goal-only prompt, no access to the PG/STDB
optimized references; it independently chose a different optimization set):
- Mongo connection pool: `maxPoolSize` 5 → 20 (default pool was the bottleneck under burst load)
- `POST /messages`: send the HTTP response right after the DB insert; defer the socket fan-out
- `trackMessageActivity`: deferred global emit via `setImmediate`; `Date[]` → `number[]`; amortised trim instead of per-message `filter()`
- `getActivityLevel`: single counting loop instead of two `filter()` allocations
- Read-only list endpoints (`GET /messages`, ephemeral cleanup): added `.lean()` (skip Mongoose hydration)
- Socket.io `perMessageDeflate: false` (compression overhead > savings for small chat payloads)
- Added compound index `{ roomId: 1, parentId: 1, createdAt: 1 }` to satisfy the room-message query+sort in one B-tree scan
- All features, validation, API/Socket.io contract, and data model: kept

## Benchmark results (averaged across 2 runs)

| Version | STDB avg | PG avg | Ratio |
|------|----------|--------|-------|
| Raw | 5,267 msgs/sec | 694 msgs/sec | 7.6x |
| Optimized (this dir) | 25,278 msgs/sec | 1,139 msgs/sec | 22x |

### MongoDB (added 20260616 — separate sitting, read caveats)

Stress throughput (writer-count sweep; peak shown), measured on the `20260616` machine:

| Version | Mongo peak | vs optimized STDB |
|------|------------|-------------------|
| Raw | ~800 msgs/sec (peak 796 @ 200 writers) | — |
| Optimized | ~1,400 msgs/sec (peak 1,394 @ 100 writers) | ~18x slower |

Optimization gain ~1.7x — in line with PG's 1.6x (both hand-built stacks gain modestly;
STDB's 4.8x reflects more architectural headroom). Optimized Mongo (~1,400) ≈ optimized
PG (1,139); both ~18–22x under optimized STDB (25,278).

**Caveats (do not drop these when citing the Mongo numbers):**
1. **Cross-machine / cross-run.** STDB & PG figures are from the original `20260406` run;
   the Mongo figures are from `20260616` on a different machine. The *within-Mongo* ratio
   (raw→optimized, ~1.7x) is clean; absolute cross-backend numbers are not strictly
   same-sitting. A same-machine re-run of all three would remove this.
2. **PG is throttle-bound.** The PG app keeps a 500ms/user send rate limit (even when
   "optimized"); the Mongo app has none. Mongo edging PG on throughput is partly that.
3. **Optimization-prompt parity.** The original PG/STDB optimization prompt was not saved.
   The Mongo pass used a reconstructed *goal-only* prompt (state the objective, let the
   model find the wins) — same spirit, not the same wording.
