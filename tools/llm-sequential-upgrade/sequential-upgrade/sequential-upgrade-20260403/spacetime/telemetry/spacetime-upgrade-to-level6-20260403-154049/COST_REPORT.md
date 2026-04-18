# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 6
**Date:** 2026-04-03
**Started:** 2026-04-03T15:40:49-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 32 |
| Total output tokens     | 16,675 |
| Total tokens            | 16,707 |
| Cache read tokens       | 2,535,312 |
| Cache creation tokens   | 40,401 |
| Total cost (USD)        | $1.1623 |
| Total API time          | 299.7s |
| API calls               | 32 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 859 | 46,701 | $0.0875 | 22.7s |
| 2 | claude-sonnet-4-6 | 1 | 911 | 62,865 | $0.0411 | 19.5s |
| 3 | claude-sonnet-4-6 | 1 | 263 | 66,092 | $0.0258 | 7.4s |
| 4 | claude-sonnet-4-6 | 1 | 233 | 66,641 | $0.0249 | 6.7s |
| 5 | claude-sonnet-4-6 | 1 | 698 | 67,297 | $0.0333 | 7.9s |
| 6 | claude-sonnet-4-6 | 1 | 1,032 | 68,010 | $0.0389 | 14.7s |
| 7 | claude-sonnet-4-6 | 1 | 233 | 68,807 | $0.0284 | 8.5s |
| 8 | claude-sonnet-4-6 | 1 | 191 | 71,448 | $0.0286 | 4.4s |
| 9 | claude-sonnet-4-6 | 1 | 3,262 | 72,592 | $0.0764 | 53.4s |
| 10 | claude-sonnet-4-6 | 1 | 242 | 74,112 | $0.0385 | 4.0s |
| 11 | claude-sonnet-4-6 | 1 | 217 | 77,481 | $0.0286 | 5.3s |
| 12 | claude-sonnet-4-6 | 1 | 233 | 78,029 | $0.0281 | 5.5s |
| 13 | claude-sonnet-4-6 | 1 | 343 | 78,667 | $0.0307 | 4.9s |
| 14 | claude-sonnet-4-6 | 1 | 773 | 79,195 | $0.0370 | 8.9s |
| 15 | claude-sonnet-4-6 | 1 | 610 | 79,632 | $0.0363 | 7.3s |
| 16 | claude-sonnet-4-6 | 1 | 355 | 80,499 | $0.0329 | 4.7s |
| 17 | claude-sonnet-4-6 | 1 | 844 | 81,415 | $0.0388 | 11.6s |
| 18 | claude-sonnet-4-6 | 1 | 172 | 81,864 | $0.0307 | 3.7s |
| 19 | claude-sonnet-4-6 | 1 | 233 | 83,939 | $0.0310 | 5.1s |
| 20 | claude-sonnet-4-6 | 1 | 176 | 84,568 | $0.0290 | 3.6s |
| 21 | claude-sonnet-4-6 | 1 | 2,098 | 84,843 | $0.0582 | 35.9s |
| 22 | claude-sonnet-4-6 | 1 | 360 | 85,196 | $0.0392 | 6.1s |
| 23 | claude-sonnet-4-6 | 1 | 197 | 87,384 | $0.0309 | 3.9s |
| 24 | claude-sonnet-4-6 | 1 | 233 | 87,838 | $0.0309 | 4.8s |
| 25 | claude-sonnet-4-6 | 1 | 162 | 88,456 | $0.0298 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 772 | 88,679 | $0.0398 | 12.2s |
| 27 | claude-sonnet-4-6 | 1 | 176 | 89,105 | $0.0334 | 4.1s |
| 28 | claude-sonnet-4-6 | 1 | 177 | 90,183 | $0.0304 | 3.7s |
| 29 | claude-sonnet-4-6 | 1 | 248 | 90,377 | $0.0326 | 4.3s |
| 30 | claude-sonnet-4-6 | 1 | 122 | 90,856 | $0.0304 | 3.4s |
| 31 | claude-sonnet-4-6 | 1 | 242 | 91,203 | $0.0315 | 6.4s |
| 32 | claude-sonnet-4-6 | 1 | 8 | 91,338 | $0.0286 | 1.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
