# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 11
**Date:** 2026-06-18
**Started:** 2026-06-17T20:40:54-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 3,356 |
| Total output tokens     | 13,797 |
| Total tokens            | 17,153 |
| Cache read tokens       | 1,720,458 |
| Cache creation tokens   | 56,119 |
| Total cost (USD)        | $1.0669 |
| Total API time          | 213.6s |
| API calls               | 24 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,389 | 16 | 0 | $0.0015 | 1.1s |
| 2 | claude-sonnet-4-6 | 3 | 308 | 20,621 | $0.1000 | 5.7s |
| 3 | claude-sonnet-4-6 | 1,943 | 216 | 35,481 | $0.0218 | 4.9s |
| 4 | claude-sonnet-4-6 | 1 | 146 | 45,519 | $0.1426 | 5.3s |
| 5 | claude-sonnet-4-6 | 1 | 6,471 | 66,644 | $0.1381 | 96.8s |
| 6 | claude-sonnet-4-6 | 1 | 542 | 70,145 | $0.0689 | 8.3s |
| 7 | claude-sonnet-4-6 | 1 | 399 | 76,771 | $0.0330 | 5.8s |
| 8 | claude-sonnet-4-6 | 1 | 195 | 77,439 | $0.0293 | 3.5s |
| 9 | claude-sonnet-4-6 | 1 | 264 | 77,964 | $0.0317 | 4.5s |
| 10 | claude-sonnet-4-6 | 1 | 214 | 78,687 | $0.0323 | 5.0s |
| 11 | claude-sonnet-4-6 | 1 | 159 | 80,290 | $0.0277 | 2.7s |
| 12 | claude-sonnet-4-6 | 1 | 904 | 80,501 | $0.0394 | 11.9s |
| 13 | claude-sonnet-4-6 | 1 | 455 | 80,781 | $0.0372 | 4.8s |
| 14 | claude-sonnet-4-6 | 1 | 251 | 81,806 | $0.0317 | 3.8s |
| 15 | claude-sonnet-4-6 | 1 | 618 | 82,363 | $0.0361 | 8.1s |
| 16 | claude-sonnet-4-6 | 1 | 494 | 82,716 | $0.0365 | 5.2s |
| 17 | claude-sonnet-4-6 | 1 | 543 | 83,436 | $0.0373 | 5.5s |
| 18 | claude-sonnet-4-6 | 1 | 610 | 84,131 | $0.0383 | 5.6s |
| 19 | claude-sonnet-4-6 | 1 | 174 | 84,776 | $0.0323 | 2.7s |
| 20 | claude-sonnet-4-6 | 1 | 170 | 85,488 | $0.0294 | 3.0s |
| 21 | claude-sonnet-4-6 | 1 | 102 | 85,680 | $0.0300 | 2.6s |
| 22 | claude-sonnet-4-6 | 1 | 173 | 86,147 | $0.0298 | 6.9s |
| 23 | claude-sonnet-4-6 | 1 | 123 | 86,367 | $0.0298 | 2.5s |
| 24 | claude-sonnet-4-6 | 1 | 250 | 86,705 | $0.0322 | 7.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
