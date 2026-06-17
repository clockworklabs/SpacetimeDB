# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-06-17
**Started:** 2026-06-17T17:47:04-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,426 |
| Total output tokens     | 37,377 |
| Total tokens            | 38,803 |
| Cache read tokens       | 2,106,878 |
| Cache creation tokens   | 82,897 |
| Total cost (USD)        | $1.6914 |
| Total API time          | 516.8s |
| API calls               | 25 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,400 | 17 | 0 | $0.0015 | 1.2s |
| 2 | claude-sonnet-4-6 | 3 | 346 | 20,621 | $0.1005 | 5.6s |
| 3 | claude-sonnet-4-6 | 1 | 233 | 35,473 | $0.0197 | 2.6s |
| 4 | claude-sonnet-4-6 | 1 | 10,721 | 43,111 | $0.2412 | 167.9s |
| 5 | claude-sonnet-4-6 | 1 | 7,571 | 54,346 | $0.2614 | 106.5s |
| 6 | claude-sonnet-4-6 | 1 | 1,331 | 76,264 | $0.1201 | 24.1s |
| 7 | claude-sonnet-4-6 | 1 | 699 | 89,132 | $0.0460 | 7.7s |
| 8 | claude-sonnet-4-6 | 1 | 341 | 90,589 | $0.0372 | 4.2s |
| 9 | claude-sonnet-4-6 | 1 | 536 | 91,414 | $0.0382 | 6.1s |
| 10 | claude-sonnet-4-6 | 1 | 163 | 91,862 | $0.0339 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 184 | 92,505 | $0.0317 | 2.7s |
| 12 | claude-sonnet-4-6 | 1 | 469 | 92,703 | $0.0373 | 7.3s |
| 13 | claude-sonnet-4-6 | 1 | 193 | 93,113 | $0.0343 | 3.1s |
| 14 | claude-sonnet-4-6 | 1 | 260 | 93,688 | $0.0340 | 3.8s |
| 15 | claude-sonnet-4-6 | 1 | 199 | 94,026 | $0.0366 | 4.3s |
| 16 | claude-sonnet-4-6 | 1 | 228 | 94,935 | $0.0345 | 3.5s |
| 17 | claude-sonnet-4-6 | 1 | 11,514 | 95,376 | $0.2034 | 120.3s |
| 18 | claude-sonnet-4-6 | 1 | 434 | 95,716 | $0.1055 | 6.4s |
| 19 | claude-sonnet-4-6 | 1 | 712 | 107,431 | $0.0462 | 7.6s |
| 20 | claude-sonnet-4-6 | 1 | 153 | 107,986 | $0.0396 | 2.8s |
| 21 | claude-sonnet-4-6 | 1 | 167 | 108,800 | $0.0363 | 2.5s |
| 22 | claude-sonnet-4-6 | 1 | 160 | 108,988 | $0.0362 | 2.3s |
| 23 | claude-sonnet-4-6 | 1 | 191 | 109,173 | $0.0384 | 7.9s |
| 24 | claude-sonnet-4-6 | 1 | 122 | 109,633 | $0.0369 | 2.8s |
| 25 | claude-sonnet-4-6 | 1 | 433 | 109,993 | $0.0409 | 10.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
