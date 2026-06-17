# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 11
**Date:** 2026-06-17
**Started:** 2026-06-17T14:02:17-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,765 |
| Total output tokens     | 17,760 |
| Total tokens            | 20,525 |
| Cache read tokens       | 1,825,550 |
| Cache creation tokens   | 68,235 |
| Total cost (USD)        | $1.2261 |
| Total API time          | 282.2s |
| API calls               | 23 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,741 | 20 | 0 | $0.0028 | 1.0s |
| 2 | claude-sonnet-4-6 | 3 | 305 | 20,621 | $0.1078 | 7.3s |
| 3 | claude-sonnet-4-6 | 1 | 205 | 36,797 | $0.0356 | 5.0s |
| 4 | claude-sonnet-4-6 | 1 | 10,157 | 47,429 | $0.3333 | 154.3s |
| 5 | claude-sonnet-4-6 | 1 | 528 | 75,218 | $0.0962 | 10.0s |
| 6 | claude-sonnet-4-6 | 1 | 271 | 86,164 | $0.0338 | 9.5s |
| 7 | claude-sonnet-4-6 | 1 | 511 | 86,816 | $0.0361 | 7.9s |
| 8 | claude-sonnet-4-6 | 1 | 198 | 87,211 | $0.0328 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 249 | 87,827 | $0.0345 | 4.0s |
| 10 | claude-sonnet-4-6 | 1 | 207 | 88,556 | $0.0350 | 3.9s |
| 11 | claude-sonnet-4-6 | 1 | 250 | 90,115 | $0.0320 | 3.8s |
| 12 | claude-sonnet-4-6 | 1 | 400 | 90,324 | $0.0349 | 4.5s |
| 13 | claude-sonnet-4-6 | 1 | 225 | 90,622 | $0.0336 | 3.6s |
| 14 | claude-sonnet-4-6 | 1 | 270 | 91,122 | $0.0333 | 3.4s |
| 15 | claude-sonnet-4-6 | 1 | 417 | 91,447 | $0.0359 | 6.1s |
| 16 | claude-sonnet-4-6 | 1 | 607 | 91,817 | $0.0403 | 8.0s |
| 17 | claude-sonnet-4-6 | 1 | 1,396 | 92,433 | $0.0529 | 19.6s |
| 18 | claude-sonnet-4-6 | 1 | 584 | 93,140 | $0.0457 | 5.6s |
| 19 | claude-sonnet-4-6 | 1 | 178 | 94,636 | $0.0352 | 4.4s |
| 20 | claude-sonnet-4-6 | 1 | 160 | 95,320 | $0.0322 | 2.4s |
| 21 | claude-sonnet-4-6 | 1 | 199 | 95,516 | $0.0344 | 3.9s |
| 22 | claude-sonnet-4-6 | 1 | 122 | 95,979 | $0.0334 | 2.4s |
| 23 | claude-sonnet-4-6 | 1 | 301 | 96,440 | $0.0343 | 8.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
