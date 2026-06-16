# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 7
**Date:** 2026-06-16
**Started:** 2026-06-16T11:52:06-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,350 |
| Total output tokens     | 27,130 |
| Total tokens            | 29,480 |
| Cache read tokens       | 1,783,302 |
| Cache creation tokens   | 67,128 |
| Total cost (USD)        | $1.1959 |
| Total API time          | 363.2s |
| API calls               | 24 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,325 | 18 | 0 | $0.0024 | 1.3s |
| 2 | claude-sonnet-4-6 | 3 | 348 | 20,501 | $0.0592 | 8.9s |
| 3 | claude-sonnet-4-6 | 1 | 200 | 33,266 | $0.0146 | 3.2s |
| 4 | claude-sonnet-4-6 | 1 | 239 | 33,707 | $0.0193 | 3.7s |
| 5 | claude-sonnet-4-6 | 1 | 6,128 | 42,643 | $0.1600 | 98.9s |
| 6 | claude-sonnet-4-6 | 1 | 3,481 | 57,387 | $0.1291 | 55.7s |
| 7 | claude-sonnet-4-6 | 1 | 566 | 73,306 | $0.0440 | 7.2s |
| 8 | claude-sonnet-4-6 | 1 | 516 | 76,905 | $0.0334 | 5.5s |
| 9 | claude-sonnet-4-6 | 1 | 259 | 77,589 | $0.0295 | 3.2s |
| 10 | claude-sonnet-4-6 | 1 | 11,339 | 78,204 | $0.1953 | 107.0s |
| 11 | claude-sonnet-4-6 | 1 | 671 | 78,661 | $0.0766 | 8.6s |
| 12 | claude-sonnet-4-6 | 1 | 329 | 90,099 | $0.0349 | 4.1s |
| 13 | claude-sonnet-4-6 | 1 | 319 | 90,888 | $0.0337 | 4.8s |
| 14 | claude-sonnet-4-6 | 1 | 172 | 91,316 | $0.0314 | 2.9s |
| 15 | claude-sonnet-4-6 | 1 | 133 | 92,267 | $0.0305 | 2.2s |
| 16 | claude-sonnet-4-6 | 1 | 190 | 92,487 | $0.0317 | 3.7s |
| 17 | claude-sonnet-4-6 | 1 | 232 | 92,786 | $0.0333 | 3.8s |
| 18 | claude-sonnet-4-6 | 1 | 270 | 93,318 | $0.0330 | 3.6s |
| 19 | claude-sonnet-4-6 | 1 | 271 | 93,563 | $0.0333 | 3.8s |
| 20 | claude-sonnet-4-6 | 1 | 269 | 93,874 | $0.0340 | 5.7s |
| 21 | claude-sonnet-4-6 | 1 | 267 | 94,357 | $0.0337 | 3.7s |
| 22 | claude-sonnet-4-6 | 1 | 227 | 94,739 | $0.0336 | 6.3s |
| 23 | claude-sonnet-4-6 | 1 | 293 | 95,223 | $0.0339 | 6.2s |
| 24 | claude-sonnet-4-6 | 1 | 393 | 96,216 | $0.0354 | 9.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
