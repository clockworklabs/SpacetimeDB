# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-06-18
**Started:** 2026-06-18T14:54:09-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,404 |
| Total output tokens     | 4,633 |
| Total tokens            | 6,037 |
| Cache read tokens       | 665,262 |
| Cache creation tokens   | 62,166 |
| Total cost (USD)        | $0.5035 |
| Total API time          | 72.9s |
| API calls               | 11 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,392 | 15 | 0 | $0.0015 | 1.3s |
| 2 | claude-sonnet-4-6 | 3 | 317 | 20,621 | $0.0677 | 5.5s |
| 3 | claude-sonnet-4-6 | 1 | 266 | 35,750 | $0.0479 | 5.8s |
| 4 | claude-sonnet-4-6 | 1 | 1,350 | 44,605 | $0.1585 | 23.6s |
| 5 | claude-sonnet-4-6 | 1 | 601 | 77,896 | $0.0379 | 7.3s |
| 6 | claude-sonnet-4-6 | 1 | 544 | 79,365 | $0.0350 | 6.3s |
| 7 | claude-sonnet-4-6 | 1 | 565 | 80,184 | $0.0349 | 7.0s |
| 8 | claude-sonnet-4-6 | 1 | 561 | 80,828 | $0.0352 | 8.1s |
| 9 | claude-sonnet-4-6 | 1 | 156 | 81,493 | $0.0293 | 2.7s |
| 10 | claude-sonnet-4-6 | 1 | 140 | 82,173 | $0.0274 | 2.2s |
| 11 | claude-sonnet-4-6 | 1 | 118 | 82,347 | $0.0281 | 3.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
