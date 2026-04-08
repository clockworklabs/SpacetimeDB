# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-07
**Started:** 2026-04-07T11:19:47-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 32 |
| Total output tokens     | 28,214 |
| Total tokens            | 28,246 |
| Cache read tokens       | 2,396,232 |
| Cache creation tokens   | 63,495 |
| Total cost (USD)        | $1.3803 |
| Total API time          | 476.3s |
| API calls               | 30 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 163 | 20,510 | $0.0419 | 10.4s |
| 2 | claude-sonnet-4-6 | 1 | 159 | 42,983 | $0.0161 | 3.1s |
| 3 | claude-sonnet-4-6 | 1 | 159 | 43,212 | $0.0232 | 3.3s |
| 4 | claude-sonnet-4-6 | 1 | 159 | 45,306 | $0.0247 | 4.8s |
| 5 | claude-sonnet-4-6 | 1 | 583 | 47,631 | $0.0288 | 9.8s |
| 6 | claude-sonnet-4-6 | 1 | 5,668 | 49,155 | $0.1091 | 82.2s |
| 7 | claude-sonnet-4-6 | 1 | 189 | 51,642 | $0.0502 | 5.3s |
| 8 | claude-sonnet-4-6 | 1 | 160 | 60,152 | $0.0296 | 7.8s |
| 9 | claude-sonnet-4-6 | 1 | 9,599 | 64,209 | $0.2062 | 150.6s |
| 10 | claude-sonnet-4-6 | 1 | 5,876 | 75,673 | $0.1475 | 81.4s |
| 11 | claude-sonnet-4-6 | 1 | 273 | 85,447 | $0.0519 | 5.3s |
| 12 | claude-sonnet-4-6 | 1 | 273 | 91,662 | $0.0337 | 3.9s |
| 13 | claude-sonnet-4-6 | 1 | 520 | 91,662 | $0.0386 | 8.7s |
| 14 | claude-sonnet-4-6 | 1 | 273 | 92,547 | $0.0342 | 4.2s |
| 15 | claude-sonnet-4-6 | 1 | 564 | 93,177 | $0.0376 | 8.2s |
| 16 | claude-sonnet-4-6 | 1 | 273 | 93,492 | $0.0346 | 4.9s |
| 17 | claude-sonnet-4-6 | 1 | 312 | 94,147 | $0.0341 | 5.9s |
| 18 | claude-sonnet-4-6 | 1 | 273 | 94,462 | $0.0339 | 7.6s |
| 19 | claude-sonnet-4-6 | 1 | 159 | 94,865 | $0.0320 | 4.6s |
| 20 | claude-sonnet-4-6 | 1 | 151 | 95,180 | $0.0315 | 3.1s |
| 21 | claude-sonnet-4-6 | 1 | 122 | 95,357 | $0.0311 | 3.1s |
| 22 | claude-sonnet-4-6 | 1 | 176 | 95,526 | $0.0323 | 4.5s |
| 23 | claude-sonnet-4-6 | 1 | 180 | 95,798 | $0.0326 | 7.0s |
| 24 | claude-sonnet-4-6 | 1 | 194 | 96,110 | $0.0331 | 5.2s |
| 25 | claude-sonnet-4-6 | 1 | 95 | 96,474 | $0.0317 | 2.7s |
| 26 | claude-sonnet-4-6 | 1 | 180 | 96,822 | $0.0334 | 5.5s |
| 27 | claude-sonnet-4-6 | 1 | 206 | 97,264 | $0.0333 | 5.9s |
| 28 | claude-sonnet-4-6 | 1 | 169 | 98,362 | $0.0327 | 3.3s |
| 29 | claude-sonnet-4-6 | 1 | 835 | 98,536 | $0.0433 | 17.6s |
| 30 | claude-sonnet-4-6 | 1 | 271 | 98,869 | $0.0373 | 6.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
