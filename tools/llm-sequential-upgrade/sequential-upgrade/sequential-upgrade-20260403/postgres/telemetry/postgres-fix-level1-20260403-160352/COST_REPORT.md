# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T16:03:52-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 12 |
| Total output tokens     | 3,711 |
| Total tokens            | 3,723 |
| Cache read tokens       | 456,249 |
| Cache creation tokens   | 18,755 |
| Total cost (USD)        | $0.2629 |
| Total API time          | 60.7s |
| API calls               | 10 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 266 | 30,161 | $0.0130 | 4.1s |
| 2 | claude-sonnet-4-6 | 1 | 516 | 33,641 | $0.0564 | 10.9s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 43,920 | $0.0251 | 5.5s |
| 4 | claude-sonnet-4-6 | 1 | 736 | 46,465 | $0.0318 | 9.4s |
| 5 | claude-sonnet-4-6 | 1 | 368 | 48,274 | $0.0232 | 5.3s |
| 6 | claude-sonnet-4-6 | 1 | 375 | 49,122 | $0.0222 | 5.3s |
| 7 | claude-sonnet-4-6 | 1 | 78 | 50,089 | $0.0169 | 2.9s |
| 8 | claude-sonnet-4-6 | 1 | 126 | 50,786 | $0.0179 | 2.2s |
| 9 | claude-sonnet-4-6 | 1 | 497 | 50,989 | $0.0296 | 8.2s |
| 10 | claude-sonnet-4-6 | 1 | 588 | 52,802 | $0.0269 | 6.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
