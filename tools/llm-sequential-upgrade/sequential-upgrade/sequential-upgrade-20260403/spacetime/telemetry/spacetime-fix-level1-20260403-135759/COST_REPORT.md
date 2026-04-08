# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T13:57:59-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 9 |
| Total output tokens     | 2,053 |
| Total tokens            | 2,062 |
| Cache read tokens       | 251,727 |
| Cache creation tokens   | 16,167 |
| Total cost (USD)        | $0.1670 |
| Total API time          | 40.7s |
| API calls               | 7 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 265 | 20,668 | $0.0544 | 4.7s |
| 2 | claude-sonnet-4-6 | 1 | 283 | 35,451 | $0.0209 | 4.9s |
| 3 | claude-sonnet-4-6 | 1 | 551 | 37,045 | $0.0235 | 9.6s |
| 4 | claude-sonnet-4-6 | 1 | 238 | 38,153 | $0.0175 | 3.6s |
| 5 | claude-sonnet-4-6 | 1 | 180 | 38,817 | $0.0161 | 3.8s |
| 6 | claude-sonnet-4-6 | 1 | 180 | 40,676 | $0.0158 | 4.5s |
| 7 | claude-sonnet-4-6 | 1 | 356 | 40,917 | $0.0188 | 9.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
