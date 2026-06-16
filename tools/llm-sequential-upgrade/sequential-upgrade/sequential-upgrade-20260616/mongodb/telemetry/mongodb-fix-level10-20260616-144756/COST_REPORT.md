# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 10
**Date:** 2026-06-16
**Started:** 2026-06-16T14:47:56-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 818 |
| Total output tokens     | 4,693 |
| Total tokens            | 5,511 |
| Cache read tokens       | 733,869 |
| Cache creation tokens   | 28,816 |
| Total cost (USD)        | $0.3993 |
| Total API time          | 84.9s |
| API calls               | 13 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 804 | 14 | 0 | $0.0009 | 2.4s |
| 2 | claude-sonnet-4-6 | 3 | 186 | 20,501 | $0.0511 | 3.4s |
| 3 | claude-sonnet-4-6 | 1 | 1,140 | 51,968 | $0.0715 | 26.4s |
| 4 | claude-sonnet-4-6 | 1 | 943 | 62,311 | $0.0388 | 8.6s |
| 5 | claude-sonnet-4-6 | 1 | 491 | 63,895 | $0.0305 | 7.6s |
| 6 | claude-sonnet-4-6 | 1 | 159 | 64,956 | $0.0245 | 4.3s |
| 7 | claude-sonnet-4-6 | 1 | 130 | 65,664 | $0.0223 | 3.5s |
| 8 | claude-sonnet-4-6 | 1 | 165 | 65,841 | $0.0228 | 2.6s |
| 9 | claude-sonnet-4-6 | 1 | 178 | 66,335 | $0.0241 | 3.3s |
| 10 | claude-sonnet-4-6 | 1 | 102 | 66,743 | $0.0228 | 2.2s |
| 11 | claude-sonnet-4-6 | 1 | 116 | 67,483 | $0.0258 | 2.4s |
| 12 | claude-sonnet-4-6 | 1 | 459 | 68,493 | $0.0319 | 9.4s |
| 13 | claude-sonnet-4-6 | 1 | 610 | 69,679 | $0.0322 | 8.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
