# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 5
**Date:** 2026-04-03
**Started:** 2026-04-03T15:18:11-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2 |
| Total output tokens     | 1,024 |
| Total tokens            | 1,026 |
| Cache read tokens       | 121,224 |
| Cache creation tokens   | 1,514 |
| Total cost (USD)        | $0.0574 |
| Total API time          | 19.0s |
| API calls               | 2 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 395 | 58,946 | $0.0280 | 6.3s |
| 2 | claude-sonnet-4-6 | 1 | 629 | 62,278 | $0.0294 | 12.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
