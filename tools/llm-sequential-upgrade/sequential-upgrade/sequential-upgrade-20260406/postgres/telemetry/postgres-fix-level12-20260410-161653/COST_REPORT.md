# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 12
**Date:** 2026-04-10
**Started:** 2026-04-10T16:16:53-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 10 |
| Total output tokens     | 2,302 |
| Total tokens            | 2,312 |
| Cache read tokens       | 337,889 |
| Cache creation tokens   | 5,053 |
| Total cost (USD)        | $0.1549 |
| Total API time          | 51.3s |
| API calls               | 10 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 344 | 29,886 | $0.0155 | 6.1s |
| 2 | claude-sonnet-4-6 | 1 | 312 | 30,252 | $0.0181 | 6.6s |
| 3 | claude-sonnet-4-6 | 1 | 246 | 32,465 | $0.0174 | 5.9s |
| 4 | claude-sonnet-4-6 | 1 | 253 | 33,518 | $0.0154 | 4.3s |
| 5 | claude-sonnet-4-6 | 1 | 195 | 33,929 | $0.0145 | 5.0s |
| 6 | claude-sonnet-4-6 | 1 | 182 | 34,288 | $0.0141 | 4.9s |
| 7 | claude-sonnet-4-6 | 1 | 115 | 34,572 | $0.0134 | 4.0s |
| 8 | claude-sonnet-4-6 | 1 | 126 | 35,117 | $0.0138 | 3.3s |
| 9 | claude-sonnet-4-6 | 1 | 485 | 35,940 | $0.0202 | 8.6s |
| 10 | claude-sonnet-4-6 | 1 | 44 | 37,922 | $0.0126 | 2.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
