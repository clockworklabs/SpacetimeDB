# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-01
**Started:** 2026-04-01T13:55:09-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 8 |
| Total output tokens     | 1,659 |
| Total tokens            | 1,667 |
| Cache read tokens       | 198,214 |
| Cache creation tokens   | 18,704 |
| Total cost (USD)        | $0.1545 |
| Total API time          | 35.5s |
| API calls               | 6 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 290 | 20,619 | $0.0506 | 7.5s |
| 2 | claude-sonnet-4-6 | 1 | 211 | 31,309 | $0.0197 | 4.2s |
| 3 | claude-sonnet-4-6 | 1 | 367 | 33,222 | $0.0250 | 6.9s |
| 4 | claude-sonnet-4-6 | 1 | 321 | 35,769 | $0.0254 | 8.1s |
| 5 | claude-sonnet-4-6 | 1 | 316 | 38,386 | $0.0182 | 4.3s |
| 6 | claude-sonnet-4-6 | 1 | 154 | 38,909 | $0.0155 | 4.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
