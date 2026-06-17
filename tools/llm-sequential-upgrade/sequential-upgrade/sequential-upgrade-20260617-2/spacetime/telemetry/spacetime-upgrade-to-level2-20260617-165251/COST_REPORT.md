# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-06-17
**Started:** 2026-06-17T16:52:51-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,399 |
| Total output tokens     | 23,516 |
| Total tokens            | 24,915 |
| Cache read tokens       | 1,153,084 |
| Cache creation tokens   | 56,030 |
| Total cost (USD)        | $1.0361 |
| Total API time          | 319.6s |
| API calls               | 20 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,378 | 15 | 0 | $0.0015 | 1.4s |
| 2 | claude-sonnet-4-6 | 3 | 321 | 20,621 | $0.0992 | 5.7s |
| 3 | claude-sonnet-4-6 | 1 | 120 | 35,319 | $0.0311 | 4.2s |
| 4 | claude-sonnet-4-6 | 1 | 8,994 | 38,436 | $0.1804 | 133.0s |
| 5 | claude-sonnet-4-6 | 1 | 976 | 44,098 | $0.1151 | 14.6s |
| 6 | claude-sonnet-4-6 | 1 | 2,486 | 58,634 | $0.0614 | 22.4s |
| 7 | claude-sonnet-4-6 | 1 | 163 | 59,717 | $0.0365 | 3.0s |
| 8 | claude-sonnet-4-6 | 1 | 184 | 62,409 | $0.0227 | 3.3s |
| 9 | claude-sonnet-4-6 | 1 | 1,191 | 62,607 | $0.0403 | 18.4s |
| 10 | claude-sonnet-4-6 | 1 | 189 | 63,208 | $0.0296 | 2.6s |
| 11 | claude-sonnet-4-6 | 1 | 261 | 64,505 | $0.0253 | 5.0s |
| 12 | claude-sonnet-4-6 | 1 | 292 | 64,839 | $0.0297 | 4.2s |
| 13 | claude-sonnet-4-6 | 1 | 387 | 65,818 | $0.0346 | 9.1s |
| 14 | claude-sonnet-4-6 | 1 | 6,052 | 67,326 | $0.1145 | 62.3s |
| 15 | claude-sonnet-4-6 | 1 | 1,203 | 67,918 | $0.0753 | 15.7s |
| 16 | claude-sonnet-4-6 | 1 | 178 | 74,072 | $0.0328 | 4.0s |
| 17 | claude-sonnet-4-6 | 1 | 161 | 75,396 | $0.0262 | 2.3s |
| 18 | claude-sonnet-4-6 | 1 | 200 | 75,592 | $0.0284 | 3.5s |
| 19 | claude-sonnet-4-6 | 1 | 122 | 76,053 | $0.0274 | 2.3s |
| 20 | claude-sonnet-4-6 | 1 | 21 | 76,516 | $0.0241 | 2.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
