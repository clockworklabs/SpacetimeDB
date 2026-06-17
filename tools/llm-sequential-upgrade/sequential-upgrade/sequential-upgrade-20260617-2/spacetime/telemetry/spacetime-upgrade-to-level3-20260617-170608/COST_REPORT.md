# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 3
**Date:** 2026-06-17
**Started:** 2026-06-17T17:06:08-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,429 |
| Total output tokens     | 23,960 |
| Total tokens            | 25,389 |
| Cache read tokens       | 1,830,076 |
| Cache creation tokens   | 61,598 |
| Total cost (USD)        | $1.2793 |
| Total API time          | 349.5s |
| API calls               | 28 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,396 | 17 | 0 | $0.0015 | 1.2s |
| 2 | claude-sonnet-4-6 | 3 | 288 | 20,621 | $0.0990 | 4.0s |
| 3 | claude-sonnet-4-6 | 1 | 120 | 35,369 | $0.0362 | 4.4s |
| 4 | claude-sonnet-4-6 | 1 | 3,307 | 39,333 | $0.1046 | 48.7s |
| 5 | claude-sonnet-4-6 | 1 | 7,836 | 46,533 | $0.1920 | 121.2s |
| 6 | claude-sonnet-4-6 | 1 | 3,368 | 56,608 | $0.1149 | 42.9s |
| 7 | claude-sonnet-4-6 | 1 | 330 | 64,502 | $0.0459 | 5.2s |
| 8 | claude-sonnet-4-6 | 1 | 432 | 68,095 | $0.0296 | 5.5s |
| 9 | claude-sonnet-4-6 | 1 | 987 | 68,551 | $0.0386 | 16.5s |
| 10 | claude-sonnet-4-6 | 1 | 204 | 69,090 | $0.0304 | 3.5s |
| 11 | claude-sonnet-4-6 | 1 | 320 | 70,184 | $0.0283 | 4.8s |
| 12 | claude-sonnet-4-6 | 1 | 75 | 70,595 | $0.0253 | 2.1s |
| 13 | claude-sonnet-4-6 | 3 | 120 | 71,086 | $0.0268 | 2.2s |
| 14 | claude-sonnet-4-6 | 1 | 139 | 71,694 | $0.0276 | 3.4s |
| 15 | claude-sonnet-4-6 | 3 | 94 | 72,369 | $0.0251 | 2.3s |
| 16 | claude-sonnet-4-6 | 1 | 205 | 72,697 | $0.0270 | 2.8s |
| 17 | claude-sonnet-4-6 | 1 | 284 | 73,056 | $0.0283 | 4.0s |
| 18 | claude-sonnet-4-6 | 1 | 312 | 73,406 | $0.0323 | 4.6s |
| 19 | claude-sonnet-4-6 | 1 | 246 | 74,343 | $0.0316 | 3.6s |
| 20 | claude-sonnet-4-6 | 1 | 2,062 | 75,272 | $0.0595 | 24.9s |
| 21 | claude-sonnet-4-6 | 1 | 329 | 76,265 | $0.0409 | 3.8s |
| 22 | claude-sonnet-4-6 | 1 | 425 | 78,448 | $0.0325 | 5.4s |
| 23 | claude-sonnet-4-6 | 1 | 378 | 78,879 | $0.0325 | 6.3s |
| 24 | claude-sonnet-4-6 | 1 | 488 | 79,406 | $0.0340 | 5.6s |
| 25 | claude-sonnet-4-6 | 1 | 531 | 79,886 | $0.0361 | 5.8s |
| 26 | claude-sonnet-4-6 | 1 | 695 | 80,575 | $0.0384 | 8.0s |
| 27 | claude-sonnet-4-6 | 1 | 195 | 81,208 | $0.0321 | 4.7s |
| 28 | claude-sonnet-4-6 | 1 | 173 | 82,005 | $0.0285 | 2.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
