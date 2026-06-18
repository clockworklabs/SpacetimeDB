# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 9
**Date:** 2026-06-18
**Started:** 2026-06-18T14:40:25-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,509 |
| Total output tokens     | 39,724 |
| Total tokens            | 41,233 |
| Cache read tokens       | 1,595,623 |
| Cache creation tokens   | 100,659 |
| Total cost (USD)        | $1.4536 |
| Total API time          | 499.4s |
| API calls               | 19 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,411 | 17 | 0 | $0.0015 | 1.5s |
| 2 | claude-sonnet-4-6 | 3 | 328 | 20,621 | $0.0680 | 5.6s |
| 3 | claude-sonnet-4-6 | 79 | 206 | 35,792 | $0.0238 | 3.9s |
| 4 | claude-sonnet-4-6 | 1 | 8,721 | 43,960 | $0.2277 | 132.2s |
| 5 | claude-sonnet-4-6 | 1 | 2,349 | 66,284 | $0.0938 | 28.0s |
| 6 | claude-sonnet-4-6 | 1 | 5,273 | 76,607 | $0.1113 | 49.6s |
| 7 | claude-sonnet-4-6 | 1 | 215 | 79,061 | $0.0471 | 7.1s |
| 8 | claude-sonnet-4-6 | 1 | 287 | 84,439 | $0.0310 | 6.2s |
| 9 | claude-sonnet-4-6 | 1 | 286 | 84,801 | $0.0347 | 5.1s |
| 10 | claude-sonnet-4-6 | 1 | 179 | 86,120 | $0.0365 | 5.6s |
| 11 | claude-sonnet-4-6 | 1 | 362 | 88,257 | $0.0510 | 6.9s |
| 12 | claude-sonnet-4-6 | 1 | 19,507 | 93,349 | $0.3235 | 210.3s |
| 13 | claude-sonnet-4-6 | 1 | 148 | 94,115 | $0.1040 | 7.1s |
| 14 | claude-sonnet-4-6 | 1 | 1,008 | 113,722 | $0.0893 | 14.7s |
| 15 | claude-sonnet-4-6 | 1 | 175 | 124,392 | $0.0442 | 3.7s |
| 16 | claude-sonnet-4-6 | 1 | 169 | 125,519 | $0.0413 | 2.7s |
| 17 | claude-sonnet-4-6 | 1 | 191 | 125,811 | $0.0424 | 3.5s |
| 18 | claude-sonnet-4-6 | 1 | 179 | 126,281 | $0.0414 | 2.7s |
| 19 | claude-sonnet-4-6 | 1 | 124 | 126,492 | $0.0411 | 3.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
