# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-06
**Started:** 2026-04-06T16:12:47-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 6 |
| Total output tokens     | 1,059 |
| Total tokens            | 1,065 |
| Cache read tokens       | 268,394 |
| Cache creation tokens   | 3,837 |
| Total cost (USD)        | $0.1108 |
| Total API time          | 34.4s |
| API calls               | 6 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 160 | 42,395 | $0.0227 | 3.3s |
| 2 | claude-sonnet-4-6 | 1 | 108 | 44,428 | $0.0156 | 7.1s |
| 3 | claude-sonnet-4-6 | 1 | 96 | 44,606 | $0.0165 | 6.2s |
| 4 | claude-sonnet-4-6 | 1 | 105 | 45,375 | $0.0159 | 3.9s |
| 5 | claude-sonnet-4-6 | 1 | 124 | 45,673 | $0.0165 | 4.0s |
| 6 | claude-sonnet-4-6 | 1 | 466 | 45,917 | $0.0236 | 9.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
