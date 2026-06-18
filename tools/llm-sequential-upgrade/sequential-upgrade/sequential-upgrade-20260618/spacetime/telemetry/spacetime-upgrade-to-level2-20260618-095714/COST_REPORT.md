# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-06-18
**Started:** 2026-06-18T09:57:14-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,395 |
| Total output tokens     | 31,199 |
| Total tokens            | 32,594 |
| Cache read tokens       | 1,172,627 |
| Cache creation tokens   | 55,447 |
| Total cost (USD)        | $1.0290 |
| Total API time          | 485.9s |
| API calls               | 18 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,376 | 15 | 0 | $0.0015 | 1.7s |
| 2 | claude-sonnet-4-6 | 3 | 451 | 20,621 | $0.0695 | 7.9s |
| 3 | claude-sonnet-4-6 | 1 | 282 | 35,701 | $0.0545 | 6.6s |
| 4 | claude-sonnet-4-6 | 1 | 146 | 46,246 | $0.0389 | 3.8s |
| 5 | claude-sonnet-4-6 | 1 | 98 | 52,345 | $0.0212 | 3.1s |
| 6 | claude-sonnet-4-6 | 1 | 15,423 | 53,408 | $0.2541 | 252.7s |
| 7 | claude-sonnet-4-6 | 1 | 1,274 | 71,005 | $0.0411 | 17.5s |
| 8 | claude-sonnet-4-6 | 1 | 2,458 | 71,200 | $0.0634 | 28.8s |
| 9 | claude-sonnet-4-6 | 1 | 209 | 72,579 | $0.0345 | 5.6s |
| 10 | claude-sonnet-4-6 | 1 | 199 | 75,142 | $0.0272 | 4.0s |
| 11 | claude-sonnet-4-6 | 1 | 301 | 75,596 | $0.0286 | 5.8s |
| 12 | claude-sonnet-4-6 | 1 | 582 | 75,980 | $0.0371 | 12.0s |
| 13 | claude-sonnet-4-6 | 1 | 8,222 | 77,453 | $0.1620 | 101.9s |
| 14 | claude-sonnet-4-6 | 1 | 889 | 81,579 | $0.0690 | 15.3s |
| 15 | claude-sonnet-4-6 | 1 | 173 | 89,901 | $0.0337 | 6.0s |
| 16 | claude-sonnet-4-6 | 1 | 166 | 91,008 | $0.0305 | 4.2s |
| 17 | claude-sonnet-4-6 | 1 | 186 | 91,199 | $0.0319 | 3.6s |
| 18 | claude-sonnet-4-6 | 1 | 125 | 91,664 | $0.0301 | 5.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
