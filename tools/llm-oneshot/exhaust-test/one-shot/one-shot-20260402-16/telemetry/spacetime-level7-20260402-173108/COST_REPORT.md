# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-04-02
**Started:** 2026-04-02T17:31:08-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,684 |
| Total output tokens     | 71,914 |
| Total tokens            | 74,598 |
| Cache read tokens       | 2,161,214 |
| Cache creation tokens   | 103,949 |
| Total cost (USD)        | $2.1249 |
| Total API time          | 969.3s |
| API calls               | 25 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 240 | 20,668 | $0.0609 | 4.2s |
| 2 | claude-sonnet-4-6 | 1 | 97 | 34,305 | $0.0128 | 2.7s |
| 3 | claude-sonnet-4-6 | 1 | 114 | 34,593 | $0.0178 | 2.2s |
| 4 | claude-sonnet-4-6 | 2,658 | 136 | 36,398 | $0.0236 | 3.7s |
| 5 | claude-sonnet-4-6 | 1 | 28,122 | 37,121 | $0.4490 | 419.3s |
| 6 | claude-sonnet-4-6 | 1 | 431 | 41,400 | $0.1252 | 4.8s |
| 7 | claude-sonnet-4-6 | 1 | 813 | 69,742 | $0.0356 | 8.6s |
| 8 | claude-sonnet-4-6 | 1 | 5,689 | 70,403 | $0.1099 | 52.4s |
| 9 | claude-sonnet-4-6 | 1 | 185 | 71,315 | $0.0459 | 3.3s |
| 10 | claude-sonnet-4-6 | 1 | 310 | 77,669 | $0.0412 | 4.7s |
| 11 | claude-sonnet-4-6 | 1 | 253 | 81,193 | $0.0452 | 5.9s |
| 12 | claude-sonnet-4-6 | 1 | 17,610 | 85,739 | $0.2979 | 217.7s |
| 13 | claude-sonnet-4-6 | 1 | 1,877 | 87,869 | $0.1228 | 36.0s |
| 14 | claude-sonnet-4-6 | 1 | 1,115 | 106,084 | $0.0557 | 12.7s |
| 15 | claude-sonnet-4-6 | 1 | 532 | 107,998 | $0.0462 | 6.6s |
| 16 | claude-sonnet-4-6 | 1 | 444 | 109,542 | $0.0424 | 7.2s |
| 17 | claude-sonnet-4-6 | 1 | 12,250 | 110,297 | $0.2189 | 138.6s |
| 18 | claude-sonnet-4-6 | 1 | 221 | 110,835 | $0.0829 | 4.5s |
| 19 | claude-sonnet-4-6 | 1 | 169 | 123,179 | $0.0405 | 4.6s |
| 20 | claude-sonnet-4-6 | 1 | 175 | 123,442 | $0.0405 | 2.5s |
| 21 | claude-sonnet-4-6 | 1 | 180 | 123,664 | $0.0405 | 4.3s |
| 22 | claude-sonnet-4-6 | 1 | 102 | 123,857 | $0.0405 | 3.9s |
| 23 | claude-sonnet-4-6 | 1 | 105 | 124,461 | $0.0397 | 2.7s |
| 24 | claude-sonnet-4-6 | 1 | 539 | 124,661 | $0.0459 | 13.0s |
| 25 | claude-sonnet-4-6 | 1 | 205 | 124,779 | $0.0435 | 3.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
