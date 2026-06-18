# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-06-18
**Started:** 2026-06-17T20:10:48-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,406 |
| Total output tokens     | 5,710 |
| Total tokens            | 7,116 |
| Cache read tokens       | 679,929 |
| Cache creation tokens   | 64,447 |
| Total cost (USD)        | $0.6776 |
| Total API time          | 89.4s |
| API calls               | 11 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,394 | 15 | 0 | $0.0015 | 1.1s |
| 2 | claude-sonnet-4-6 | 3 | 347 | 20,621 | $0.1006 | 8.9s |
| 3 | claude-sonnet-4-6 | 1 | 271 | 35,488 | $0.0729 | 8.9s |
| 4 | claude-sonnet-4-6 | 1 | 1,960 | 45,187 | $0.2469 | 32.7s |
| 5 | claude-sonnet-4-6 | 1 | 837 | 79,178 | $0.0488 | 8.1s |
| 6 | claude-sonnet-4-6 | 1 | 581 | 81,259 | $0.0394 | 6.1s |
| 7 | claude-sonnet-4-6 | 1 | 758 | 82,316 | $0.0402 | 7.6s |
| 8 | claude-sonnet-4-6 | 1 | 453 | 82,999 | $0.0369 | 5.3s |
| 9 | claude-sonnet-4-6 | 1 | 175 | 83,859 | $0.0311 | 2.9s |
| 10 | claude-sonnet-4-6 | 1 | 160 | 84,414 | $0.0289 | 3.6s |
| 11 | claude-sonnet-4-6 | 1 | 153 | 84,608 | $0.0304 | 4.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
