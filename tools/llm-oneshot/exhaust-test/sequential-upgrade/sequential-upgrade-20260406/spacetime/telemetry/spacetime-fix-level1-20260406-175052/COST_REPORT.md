# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-06
**Started:** 2026-04-06T17:50:52-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 19 |
| Total output tokens     | 16,909 |
| Total tokens            | 16,928 |
| Cache read tokens       | 1,167,489 |
| Cache creation tokens   | 54,231 |
| Total cost (USD)        | $0.8073 |
| Total API time          | 266.2s |
| API calls               | 17 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 167 | 20,510 | $0.0482 | 3.0s |
| 2 | claude-sonnet-4-6 | 1 | 245 | 20,510 | $0.0511 | 3.2s |
| 3 | claude-sonnet-4-6 | 1 | 392 | 31,043 | $0.0418 | 8.3s |
| 4 | claude-sonnet-4-6 | 1 | 376 | 47,339 | $0.0306 | 4.7s |
| 5 | claude-sonnet-4-6 | 1 | 217 | 50,207 | $0.0227 | 3.2s |
| 6 | claude-sonnet-4-6 | 1 | 1,030 | 51,362 | $0.0450 | 18.6s |
| 7 | claude-sonnet-4-6 | 1 | 6,525 | 55,130 | $0.1222 | 95.4s |
| 8 | claude-sonnet-4-6 | 1 | 1,007 | 57,200 | $0.0585 | 17.3s |
| 9 | claude-sonnet-4-6 | 1 | 166 | 64,188 | $0.0257 | 2.6s |
| 10 | claude-sonnet-4-6 | 1 | 162 | 65,238 | $0.0230 | 3.5s |
| 11 | claude-sonnet-4-6 | 1 | 5,211 | 72,282 | $0.1009 | 80.6s |
| 12 | claude-sonnet-4-6 | 1 | 144 | 95,959 | $0.0515 | 4.2s |
| 13 | claude-sonnet-4-6 | 1 | 223 | 105,785 | $0.0371 | 3.8s |
| 14 | claude-sonnet-4-6 | 1 | 200 | 106,333 | $0.0364 | 2.8s |
| 15 | claude-sonnet-4-6 | 1 | 227 | 106,728 | $0.0366 | 3.5s |
| 16 | claude-sonnet-4-6 | 1 | 125 | 108,713 | $0.0354 | 2.4s |
| 17 | claude-sonnet-4-6 | 1 | 492 | 108,962 | $0.0407 | 9.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
