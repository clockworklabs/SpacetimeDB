# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 8
**Date:** 2026-04-03
**Started:** 2026-04-03T17:47:18-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 4 |
| Total output tokens     | 1,679 |
| Total tokens            | 1,683 |
| Cache read tokens       | 192,498 |
| Cache creation tokens   | 19,788 |
| Total cost (USD)        | $0.1572 |
| Total API time          | 37.0s |
| API calls               | 4 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 364 | 33,937 | $0.0582 | 7.8s |
| 2 | claude-sonnet-4-6 | 1 | 274 | 45,282 | $0.0325 | 7.7s |
| 3 | claude-sonnet-4-6 | 1 | 854 | 53,365 | $0.0399 | 14.3s |
| 4 | claude-sonnet-4-6 | 1 | 187 | 59,914 | $0.0266 | 7.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
