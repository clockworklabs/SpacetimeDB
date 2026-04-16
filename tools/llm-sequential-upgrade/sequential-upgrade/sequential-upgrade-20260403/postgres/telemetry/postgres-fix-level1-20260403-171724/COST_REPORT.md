# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T17:17:24-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 21 |
| Total output tokens     | 28,698 |
| Total tokens            | 28,719 |
| Cache read tokens       | 1,491,670 |
| Cache creation tokens   | 28,857 |
| Total cost (USD)        | $0.9862 |
| Total API time          | 476.5s |
| API calls               | 19 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 268 | 20,668 | $0.0451 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 618 | 48,478 | $0.0292 | 9.9s |
| 3 | claude-sonnet-4-6 | 1 | 3,558 | 55,871 | $0.0826 | 59.7s |
| 4 | claude-sonnet-4-6 | 1 | 2,453 | 59,191 | $0.0702 | 40.2s |
| 5 | claude-sonnet-4-6 | 1 | 6,873 | 63,364 | $0.1385 | 110.0s |
| 6 | claude-sonnet-4-6 | 1 | 11,496 | 75,049 | $0.1972 | 193.0s |
| 7 | claude-sonnet-4-6 | 1 | 195 | 87,161 | $0.0308 | 2.8s |
| 8 | claude-sonnet-4-6 | 1 | 248 | 87,612 | $0.0309 | 3.9s |
| 9 | claude-sonnet-4-6 | 1 | 183 | 87,849 | $0.0305 | 4.6s |
| 10 | claude-sonnet-4-6 | 1 | 390 | 88,209 | $0.0344 | 5.2s |
| 11 | claude-sonnet-4-6 | 1 | 176 | 88,763 | $0.0311 | 2.9s |
| 12 | claude-sonnet-4-6 | 1 | 386 | 89,246 | $0.0344 | 4.2s |
| 13 | claude-sonnet-4-6 | 1 | 195 | 89,746 | $0.0316 | 4.4s |
| 14 | claude-sonnet-4-6 | 1 | 104 | 90,594 | $0.0294 | 3.6s |
| 15 | claude-sonnet-4-6 | 1 | 224 | 91,330 | $0.0313 | 3.6s |
| 16 | claude-sonnet-4-6 | 1 | 160 | 91,476 | $0.0308 | 2.6s |
| 17 | claude-sonnet-4-6 | 1 | 783 | 91,476 | $0.0425 | 12.4s |
| 18 | claude-sonnet-4-6 | 1 | 193 | 92,356 | $0.0339 | 3.0s |
| 19 | claude-sonnet-4-6 | 1 | 195 | 93,231 | $0.0318 | 6.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
