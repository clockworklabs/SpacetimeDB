# Perf Benchmark Summary - PG vs STDB Chat Apps

Runtime performance of the **Level 11 chat apps the LLM built** in the sequential upgrade benchmark.
Both apps run on the same dev machine against a local DB. Numbers reflect what shipped, not the theoretical ceiling of either backend.

## stress-throughput

| Metric | PostgreSQL | SpacetimeDB |
|---|---|---|
| Sustained throughput (msgs/sec) | 60.0 | 2327.0 |
| Messages received | 1800 | 69811 |
| Fan-out latency p50 (ms) | 27.3 | 10.2 |
| Fan-out latency p99 (ms) | 47.3 | 28.0 |
| Ack latency p50 (ms) | 27.5 | 12.8 |
| Ack latency p99 (ms) | 48.3 | 29.8 |

**PG note:** 30 writers firing as fast as possible

## realistic-chat

| Metric | PostgreSQL | SpacetimeDB |
|---|---|---|
| Sustained throughput (msgs/sec) | 5.2 | 5.3 |
| Messages received | 942 | 320 |
| Fan-out latency p50 (ms) | 7.0 | 4.4 |
| Fan-out latency p99 (ms) | 59.4 | 423.9 |

**PG note:** 50 users, jitter 5000-15000ms

**STDB note:** 50 users, jitter 5000-15000ms

## Headline

Under stress, the SpacetimeDB app delivered **39x the throughput** of the PostgreSQL app 
(2327.0 vs 60.0 msgs/sec)
with comparable p99 fan-out latency (28.0ms vs 47.3ms).

The PG send_message handler serializes 5 DB queries per message (ban check, membership check,
`lastSeen` update, insert, roomMembers query for notifications) - all awaited, no batching.
The SpacetimeDB reducer does a single transaction. **This is what shipped from the same prompt** -
the LLM reached for a familiar REST pattern on PG and a minimal reducer on STDB, and the
generated code's structure dominates the throughput gap.