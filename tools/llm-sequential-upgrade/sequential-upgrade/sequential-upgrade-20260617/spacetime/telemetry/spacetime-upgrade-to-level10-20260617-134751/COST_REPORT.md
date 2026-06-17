# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-06-17
**Started:** 2026-06-17T13:47:51-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,754 |
| Total output tokens     | 10,266 |
| Total tokens            | 13,020 |
| Cache read tokens       | 1,018,509 |
| Cache creation tokens   | 60,336 |
| Total cost (USD)        | $0.8243 |
| Total API time          | 158.3s |
| API calls               | 15 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,683 | 20 | 0 | $0.0028 | 1.4s |
| 2 | claude-sonnet-4-6 | 3 | 350 | 20,621 | $0.1076 | 7.4s |
| 3 | claude-sonnet-4-6 | 56 | 217 | 36,653 | $0.0167 | 2.8s |
| 4 | claude-sonnet-4-6 | 1 | 250 | 37,039 | $0.0375 | 5.7s |
| 5 | claude-sonnet-4-6 | 1 | 1,545 | 51,536 | $0.1581 | 26.6s |
| 6 | claude-sonnet-4-6 | 1 | 4,808 | 71,448 | $0.1620 | 67.7s |
| 7 | claude-sonnet-4-6 | 1 | 471 | 82,856 | $0.0615 | 6.2s |
| 8 | claude-sonnet-4-6 | 1 | 509 | 87,783 | $0.0375 | 7.1s |
| 9 | claude-sonnet-4-6 | 1 | 583 | 88,373 | $0.0389 | 6.1s |
| 10 | claude-sonnet-4-6 | 1 | 575 | 88,982 | $0.0394 | 7.0s |
| 11 | claude-sonnet-4-6 | 1 | 175 | 89,665 | $0.0336 | 3.1s |
| 12 | claude-sonnet-4-6 | 1 | 159 | 90,340 | $0.0312 | 2.6s |
| 13 | claude-sonnet-4-6 | 1 | 229 | 90,633 | $0.0334 | 3.9s |
| 14 | claude-sonnet-4-6 | 1 | 174 | 91,093 | $0.0323 | 3.0s |
| 15 | claude-sonnet-4-6 | 1 | 201 | 91,487 | $0.0316 | 7.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
