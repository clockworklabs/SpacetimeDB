# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 8
**Date:** 2026-06-16
**Started:** 2026-06-16T13:58:50-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 817 |
| Total output tokens     | 3,278 |
| Total tokens            | 4,095 |
| Cache read tokens       | 585,973 |
| Cache creation tokens   | 40,922 |
| Total cost (USD)        | $0.3791 |
| Total API time          | 73.5s |
| API calls               | 12 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 804 | 14 | 0 | $0.0009 | 1.2s |
| 2 | claude-sonnet-4-6 | 3 | 178 | 20,501 | $0.0514 | 4.0s |
| 3 | claude-sonnet-4-6 | 1 | 1,118 | 32,245 | $0.1198 | 21.8s |
| 4 | claude-sonnet-4-6 | 1 | 184 | 57,126 | $0.0245 | 5.6s |
| 5 | claude-sonnet-4-6 | 1 | 105 | 58,362 | $0.0203 | 2.2s |
| 6 | claude-sonnet-4-6 | 1 | 180 | 58,680 | $0.0208 | 4.4s |
| 7 | claude-sonnet-4-6 | 1 | 91 | 58,817 | $0.0203 | 1.9s |
| 8 | claude-sonnet-4-6 | 1 | 219 | 59,163 | $0.0225 | 4.6s |
| 9 | claude-sonnet-4-6 | 1 | 102 | 59,547 | $0.0208 | 5.9s |
| 10 | claude-sonnet-4-6 | 1 | 129 | 59,932 | $0.0214 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 422 | 60,331 | $0.0279 | 8.6s |
| 12 | claude-sonnet-4-6 | 1 | 536 | 61,269 | $0.0285 | 10.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
