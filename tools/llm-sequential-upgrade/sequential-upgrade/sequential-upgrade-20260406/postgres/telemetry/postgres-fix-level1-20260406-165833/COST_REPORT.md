# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-06
**Started:** 2026-04-06T16:58:34-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 11 |
| Total output tokens     | 2,600 |
| Total tokens            | 2,611 |
| Cache read tokens       | 351,479 |
| Cache creation tokens   | 14,782 |
| Total cost (USD)        | $0.1999 |
| Total API time          | 52.1s |
| API calls               | 9 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 167 | 20,510 | $0.0428 | 3.2s |
| 2 | claude-sonnet-4-6 | 1 | 106 | 38,487 | $0.0153 | 3.5s |
| 3 | claude-sonnet-4-6 | 1 | 705 | 39,266 | $0.0266 | 13.7s |
| 4 | claude-sonnet-4-6 | 1 | 581 | 40,409 | $0.0243 | 9.9s |
| 5 | claude-sonnet-4-6 | 1 | 224 | 41,338 | $0.0180 | 4.8s |
| 6 | claude-sonnet-4-6 | 1 | 97 | 41,933 | $0.0157 | 2.4s |
| 7 | claude-sonnet-4-6 | 1 | 107 | 42,710 | $0.0152 | 4.3s |
| 8 | claude-sonnet-4-6 | 1 | 124 | 43,413 | $0.0163 | 2.7s |
| 9 | claude-sonnet-4-6 | 1 | 489 | 43,413 | $0.0256 | 7.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
