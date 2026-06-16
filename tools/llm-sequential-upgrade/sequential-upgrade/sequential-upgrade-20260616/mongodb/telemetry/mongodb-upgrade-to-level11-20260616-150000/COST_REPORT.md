# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 11
**Date:** 2026-06-16
**Started:** 2026-06-16T15:00:00-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,605 |
| Total output tokens     | 18,417 |
| Total tokens            | 21,022 |
| Cache read tokens       | 2,021,318 |
| Cache creation tokens   | 41,676 |
| Total cost (USD)        | $1.0415 |
| Total API time          | 275.4s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,580 | 14 | 0 | $0.0027 | 2.0s |
| 2 | claude-sonnet-4-6 | 1 | 9,387 | 46,196 | $0.2296 | 136.5s |
| 3 | claude-sonnet-4-6 | 1 | 199 | 66,170 | $0.0585 | 4.3s |
| 4 | claude-sonnet-4-6 | 1 | 366 | 75,675 | $0.0294 | 7.7s |
| 5 | claude-sonnet-4-6 | 1 | 481 | 75,992 | $0.0321 | 6.5s |
| 6 | claude-sonnet-4-6 | 1 | 956 | 76,556 | $0.0395 | 10.8s |
| 7 | claude-sonnet-4-6 | 1 | 383 | 77,136 | $0.0328 | 5.4s |
| 8 | claude-sonnet-4-6 | 1 | 808 | 78,191 | $0.0374 | 11.4s |
| 9 | claude-sonnet-4-6 | 1 | 1,234 | 78,673 | $0.0455 | 13.4s |
| 10 | claude-sonnet-4-6 | 1 | 440 | 79,580 | $0.0358 | 6.4s |
| 11 | claude-sonnet-4-6 | 1 | 687 | 81,012 | $0.0366 | 7.0s |
| 12 | claude-sonnet-4-6 | 1 | 431 | 81,551 | $0.0339 | 4.5s |
| 13 | claude-sonnet-4-6 | 1 | 526 | 82,337 | $0.0346 | 7.8s |
| 14 | claude-sonnet-4-6 | 1 | 447 | 82,867 | $0.0339 | 6.2s |
| 15 | claude-sonnet-4-6 | 1 | 167 | 83,492 | $0.0300 | 4.1s |
| 16 | claude-sonnet-4-6 | 1 | 151 | 84,834 | $0.0284 | 4.1s |
| 17 | claude-sonnet-4-6 | 1 | 295 | 85,024 | $0.0313 | 3.5s |
| 18 | claude-sonnet-4-6 | 1 | 157 | 85,378 | $0.0294 | 2.7s |
| 19 | claude-sonnet-4-6 | 1 | 153 | 85,772 | $0.0287 | 3.2s |
| 20 | claude-sonnet-4-6 | 1 | 114 | 85,947 | $0.0292 | 3.0s |
| 21 | claude-sonnet-4-6 | 1 | 162 | 86,390 | $0.0298 | 3.3s |
| 22 | claude-sonnet-4-6 | 1 | 177 | 87,093 | $0.0313 | 3.5s |
| 23 | claude-sonnet-4-6 | 1 | 164 | 87,766 | $0.0313 | 3.3s |
| 24 | claude-sonnet-4-6 | 1 | 87 | 88,656 | $0.0286 | 2.1s |
| 25 | claude-sonnet-4-6 | 1 | 98 | 89,325 | $0.0289 | 2.4s |
| 26 | claude-sonnet-4-6 | 1 | 333 | 89,705 | $0.0324 | 10.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
