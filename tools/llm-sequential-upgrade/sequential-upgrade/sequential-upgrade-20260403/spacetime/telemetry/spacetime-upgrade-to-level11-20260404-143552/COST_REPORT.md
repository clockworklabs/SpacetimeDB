# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 11
**Date:** 2026-04-04
**Started:** 2026-04-04T14:35:52-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 30 |
| Total output tokens     | 10,266 |
| Total tokens            | 10,296 |
| Cache read tokens       | 2,307,895 |
| Cache creation tokens   | 41,128 |
| Total cost (USD)        | $1.0007 |
| Total API time          | 204.8s |
| API calls               | 30 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 295 | 36,834 | $0.0306 | 7.8s |
| 2 | claude-sonnet-4-6 | 1 | 145 | 51,320 | $0.0185 | 3.0s |
| 3 | claude-sonnet-4-6 | 1 | 162 | 51,555 | $0.0249 | 4.3s |
| 4 | claude-sonnet-4-6 | 1 | 162 | 51,555 | $0.0341 | 4.5s |
| 5 | claude-sonnet-4-6 | 1 | 162 | 55,881 | $0.0320 | 6.7s |
| 6 | claude-sonnet-4-6 | 1 | 162 | 59,295 | $0.0306 | 5.8s |
| 7 | claude-sonnet-4-6 | 1 | 162 | 62,059 | $0.0325 | 5.2s |
| 8 | claude-sonnet-4-6 | 1 | 572 | 68,627 | $0.0490 | 12.7s |
| 9 | claude-sonnet-4-6 | 1 | 471 | 73,927 | $0.0330 | 7.5s |
| 10 | claude-sonnet-4-6 | 1 | 318 | 74,915 | $0.0356 | 6.1s |
| 11 | claude-sonnet-4-6 | 1 | 835 | 77,145 | $0.0423 | 17.7s |
| 12 | claude-sonnet-4-6 | 1 | 563 | 78,911 | $0.0393 | 9.4s |
| 13 | claude-sonnet-4-6 | 1 | 254 | 81,419 | $0.0310 | 4.8s |
| 14 | claude-sonnet-4-6 | 1 | 485 | 82,156 | $0.0330 | 9.6s |
| 15 | claude-sonnet-4-6 | 1 | 254 | 82,452 | $0.0308 | 3.7s |
| 16 | claude-sonnet-4-6 | 1 | 254 | 83,351 | $0.0316 | 5.7s |
| 17 | claude-sonnet-4-6 | 1 | 254 | 84,388 | $0.0327 | 5.7s |
| 18 | claude-sonnet-4-6 | 1 | 149 | 85,345 | $0.0290 | 3.8s |
| 19 | claude-sonnet-4-6 | 1 | 376 | 86,104 | $0.0324 | 8.4s |
| 20 | claude-sonnet-4-6 | 1 | 217 | 86,341 | $0.0310 | 6.0s |
| 21 | claude-sonnet-4-6 | 1 | 240 | 86,830 | $0.0308 | 7.4s |
| 22 | claude-sonnet-4-6 | 1 | 609 | 87,141 | $0.0365 | 9.1s |
| 23 | claude-sonnet-4-6 | 1 | 868 | 87,475 | $0.0426 | 10.4s |
| 24 | claude-sonnet-4-6 | 1 | 746 | 88,377 | $0.0413 | 9.8s |
| 25 | claude-sonnet-4-6 | 1 | 249 | 89,339 | $0.0337 | 4.5s |
| 26 | claude-sonnet-4-6 | 1 | 447 | 90,179 | $0.0350 | 6.0s |
| 27 | claude-sonnet-4-6 | 1 | 260 | 90,522 | $0.0331 | 6.8s |
| 28 | claude-sonnet-4-6 | 1 | 175 | 91,063 | $0.0311 | 3.8s |
| 29 | claude-sonnet-4-6 | 1 | 168 | 91,365 | $0.0307 | 4.3s |
| 30 | claude-sonnet-4-6 | 1 | 252 | 92,024 | $0.0320 | 4.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
