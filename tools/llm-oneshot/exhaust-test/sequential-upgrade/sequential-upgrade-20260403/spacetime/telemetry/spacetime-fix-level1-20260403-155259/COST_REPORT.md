# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T15:52:59-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 12 |
| Total output tokens     | 3,758 |
| Total tokens            | 3,770 |
| Cache read tokens       | 387,516 |
| Cache creation tokens   | 5,593 |
| Total cost (USD)        | $0.1936 |
| Total API time          | 56.6s |
| API calls               | 10 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 285 | 32,447 | $0.0140 | 4.1s |
| 2 | claude-sonnet-4-6 | 1 | 191 | 34,490 | $0.0141 | 3.0s |
| 3 | claude-sonnet-4-6 | 1 | 386 | 34,490 | $0.0205 | 6.5s |
| 4 | claude-sonnet-4-6 | 1 | 1,060 | 37,498 | $0.0319 | 12.8s |
| 5 | claude-sonnet-4-6 | 1 | 599 | 40,129 | $0.0224 | 6.3s |
| 6 | claude-sonnet-4-6 | 1 | 185 | 40,488 | $0.0176 | 3.8s |
| 7 | claude-sonnet-4-6 | 1 | 186 | 41,403 | $0.0157 | 4.2s |
| 8 | claude-sonnet-4-6 | 1 | 162 | 41,538 | $0.0174 | 3.2s |
| 9 | claude-sonnet-4-6 | 1 | 172 | 42,198 | $0.0176 | 3.2s |
| 10 | claude-sonnet-4-6 | 1 | 532 | 42,835 | $0.0224 | 9.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
