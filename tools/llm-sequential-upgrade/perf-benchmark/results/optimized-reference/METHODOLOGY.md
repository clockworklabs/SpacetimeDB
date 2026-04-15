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

## Benchmark results (averaged across 2 runs)

| Version | STDB avg | PG avg | Ratio |
|------|----------|--------|-------|
| Raw | 5,267 msgs/sec | 694 msgs/sec | 7.6x |
| Optimized (this dir) | 25,278 msgs/sec | 1,139 msgs/sec | 22x |