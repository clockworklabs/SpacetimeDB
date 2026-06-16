# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 4
**Date:** 2026-06-16
**Started:** 2026-06-16T11:15:39-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,166 |
| Total output tokens     | 9,406 |
| Total tokens            | 11,572 |
| Cache read tokens       | 927,471 |
| Cache creation tokens   | 43,280 |
| Total cost (USD)        | $0.5837 |
| Total API time          | 154.9s |
| API calls               | 18 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,147 | 14 | 0 | $0.0022 | 1.8s |
| 2 | claude-sonnet-4-6 | 3 | 404 | 20,501 | $0.0596 | 14.7s |
| 3 | claude-sonnet-4-6 | 1 | 157 | 33,128 | $0.0349 | 6.9s |
| 4 | claude-sonnet-4-6 | 1 | 676 | 39,146 | $0.0523 | 18.0s |
| 5 | claude-sonnet-4-6 | 1 | 2,587 | 47,246 | $0.0802 | 35.9s |
| 6 | claude-sonnet-4-6 | 1 | 610 | 54,516 | $0.0357 | 7.6s |
| 7 | claude-sonnet-4-6 | 1 | 316 | 57,221 | $0.0250 | 4.3s |
| 8 | claude-sonnet-4-6 | 1 | 337 | 58,048 | $0.0241 | 5.3s |
| 9 | claude-sonnet-4-6 | 1 | 483 | 58,482 | $0.0264 | 6.8s |
| 10 | claude-sonnet-4-6 | 1 | 359 | 58,918 | $0.0252 | 4.1s |
| 11 | claude-sonnet-4-6 | 1 | 1,501 | 59,500 | $0.0421 | 14.5s |
| 12 | claude-sonnet-4-6 | 1 | 817 | 59,958 | $0.0366 | 9.9s |
| 13 | claude-sonnet-4-6 | 1 | 329 | 61,657 | $0.0269 | 4.1s |
| 14 | claude-sonnet-4-6 | 1 | 170 | 62,573 | $0.0228 | 2.8s |
| 15 | claude-sonnet-4-6 | 1 | 137 | 63,433 | $0.0220 | 3.2s |
| 16 | claude-sonnet-4-6 | 1 | 109 | 64,094 | $0.0216 | 4.3s |
| 17 | claude-sonnet-4-6 | 1 | 124 | 64,418 | $0.0220 | 2.3s |
| 18 | claude-sonnet-4-6 | 1 | 276 | 64,632 | $0.0240 | 8.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
