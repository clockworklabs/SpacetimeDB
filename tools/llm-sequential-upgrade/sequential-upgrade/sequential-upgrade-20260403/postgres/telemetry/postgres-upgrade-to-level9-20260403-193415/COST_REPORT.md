# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 9
**Date:** 2026-04-03
**Started:** 2026-04-03T19:34:15-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 19 |
| Total output tokens     | 6,315 |
| Total tokens            | 6,334 |
| Cache read tokens       | 802,660 |
| Cache creation tokens   | 46,647 |
| Total cost (USD)        | $0.5105 |
| Total API time          | 141.4s |
| API calls               | 17 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 302 | 20,668 | $0.0509 | 6.4s |
| 2 | claude-sonnet-4-6 | 1 | 249 | 33,005 | $0.0158 | 3.5s |
| 3 | claude-sonnet-4-6 | 1 | 269 | 33,005 | $0.0231 | 4.4s |
| 4 | claude-sonnet-4-6 | 1 | 246 | 35,455 | $0.0157 | 4.8s |
| 5 | claude-sonnet-4-6 | 1 | 161 | 35,828 | $0.0200 | 5.3s |
| 6 | claude-sonnet-4-6 | 1 | 161 | 35,828 | $0.0305 | 6.0s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 40,451 | $0.0269 | 5.5s |
| 8 | claude-sonnet-4-6 | 1 | 161 | 43,748 | $0.0275 | 6.0s |
| 9 | claude-sonnet-4-6 | 1 | 963 | 46,947 | $0.0368 | 18.3s |
| 10 | claude-sonnet-4-6 | 1 | 161 | 49,155 | $0.0258 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 161 | 51,452 | $0.0261 | 6.6s |
| 12 | claude-sonnet-4-6 | 1 | 161 | 53,664 | $0.0269 | 7.3s |
| 13 | claude-sonnet-4-6 | 1 | 465 | 55,897 | $0.0346 | 11.9s |
| 14 | claude-sonnet-4-6 | 1 | 1,581 | 62,182 | $0.0599 | 29.5s |
| 15 | claude-sonnet-4-6 | 1 | 864 | 66,856 | $0.0404 | 15.2s |
| 16 | claude-sonnet-4-6 | 1 | 89 | 68,826 | $0.0252 | 3.6s |
| 17 | claude-sonnet-4-6 | 1 | 160 | 69,693 | $0.0242 | 3.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
