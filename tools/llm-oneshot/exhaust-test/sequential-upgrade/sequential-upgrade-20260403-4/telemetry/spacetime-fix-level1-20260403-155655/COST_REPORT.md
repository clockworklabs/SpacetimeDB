# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T15:56:55-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 6 |
| Total output tokens     | 2,572 |
| Total tokens            | 2,578 |
| Cache read tokens       | 243,690 |
| Cache creation tokens   | 4,520 |
| Total cost (USD)        | $0.1287 |
| Total API time          | 52.7s |
| API calls               | 6 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 195 | 32,487 | $0.0135 | 3.7s |
| 2 | claude-sonnet-4-6 | 1 | 1,384 | 36,305 | $0.0397 | 21.6s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 40,718 | $0.0199 | 5.9s |
| 4 | claude-sonnet-4-6 | 1 | 160 | 44,048 | $0.0171 | 7.3s |
| 5 | claude-sonnet-4-6 | 1 | 163 | 44,991 | $0.0165 | 5.4s |
| 6 | claude-sonnet-4-6 | 1 | 509 | 45,141 | $0.0219 | 8.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
