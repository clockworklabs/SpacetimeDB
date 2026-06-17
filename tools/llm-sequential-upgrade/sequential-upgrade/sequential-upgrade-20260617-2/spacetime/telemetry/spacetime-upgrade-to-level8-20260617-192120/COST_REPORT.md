# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 8
**Date:** 2026-06-17
**Started:** 2026-06-17T19:21:20-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,416 |
| Total output tokens     | 17,810 |
| Total tokens            | 19,226 |
| Cache read tokens       | 3,043,534 |
| Cache creation tokens   | 70,564 |
| Total cost (USD)        | $1.6049 |
| Total API time          | 273.9s |
| API calls               | 35 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,382 | 17 | 0 | $0.0015 | 1.0s |
| 2 | claude-sonnet-4-6 | 1 | 216 | 38,795 | $0.0174 | 2.9s |
| 3 | claude-sonnet-4-6 | 1 | 206 | 39,210 | $0.0408 | 4.1s |
| 4 | claude-sonnet-4-6 | 1 | 295 | 43,537 | $0.0668 | 5.5s |
| 5 | claude-sonnet-4-6 | 1 | 2,194 | 51,753 | $0.1329 | 40.0s |
| 6 | claude-sonnet-4-6 | 1 | 964 | 65,838 | $0.0554 | 14.4s |
| 7 | claude-sonnet-4-6 | 1 | 1,494 | 69,368 | $0.1209 | 25.1s |
| 8 | claude-sonnet-4-6 | 1 | 326 | 82,314 | $0.0393 | 4.2s |
| 9 | claude-sonnet-4-6 | 1 | 330 | 83,934 | $0.0328 | 4.3s |
| 10 | claude-sonnet-4-6 | 1 | 329 | 84,386 | $0.0329 | 4.3s |
| 11 | claude-sonnet-4-6 | 1 | 702 | 84,823 | $0.0392 | 11.2s |
| 12 | claude-sonnet-4-6 | 1 | 252 | 85,358 | $0.0342 | 4.4s |
| 13 | claude-sonnet-4-6 | 1 | 548 | 86,167 | $0.0369 | 9.6s |
| 14 | claude-sonnet-4-6 | 1 | 168 | 86,641 | $0.0328 | 2.5s |
| 15 | claude-sonnet-4-6 | 1 | 335 | 87,360 | $0.0328 | 5.2s |
| 16 | claude-sonnet-4-6 | 1 | 268 | 87,624 | $0.0335 | 3.9s |
| 17 | claude-sonnet-4-6 | 1 | 243 | 88,148 | $0.0332 | 3.9s |
| 18 | claude-sonnet-4-6 | 1 | 278 | 88,659 | $0.0362 | 3.9s |
| 19 | claude-sonnet-4-6 | 1 | 215 | 89,557 | $0.0361 | 3.5s |
| 20 | claude-sonnet-4-6 | 1 | 452 | 94,773 | $0.0630 | 8.2s |
| 21 | claude-sonnet-4-6 | 1 | 265 | 99,411 | $0.0377 | 3.6s |
| 22 | claude-sonnet-4-6 | 1 | 615 | 100,431 | $0.0426 | 7.4s |
| 23 | claude-sonnet-4-6 | 1 | 186 | 100,974 | $0.0374 | 3.2s |
| 24 | claude-sonnet-4-6 | 1 | 1,178 | 101,691 | $0.0583 | 16.4s |
| 25 | claude-sonnet-4-6 | 1 | 614 | 103,371 | $0.0485 | 8.2s |
| 26 | claude-sonnet-4-6 | 1 | 1,020 | 104,750 | $0.0510 | 14.4s |
| 27 | claude-sonnet-4-6 | 1 | 154 | 105,466 | $0.0467 | 2.7s |
| 28 | claude-sonnet-4-6 | 1 | 277 | 107,587 | $0.0425 | 4.8s |
| 29 | claude-sonnet-4-6 | 1 | 1,112 | 108,599 | $0.0546 | 15.7s |
| 30 | claude-sonnet-4-6 | 1 | 1,805 | 109,489 | $0.0672 | 20.7s |
| 31 | claude-sonnet-4-6 | 1 | 187 | 110,703 | $0.0481 | 3.4s |
| 32 | claude-sonnet-4-6 | 1 | 178 | 112,709 | $0.0377 | 3.2s |
| 33 | claude-sonnet-4-6 | 1 | 235 | 112,914 | $0.0403 | 3.7s |
| 34 | claude-sonnet-4-6 | 1 | 128 | 113,394 | $0.0384 | 2.4s |
| 35 | claude-sonnet-4-6 | 1 | 24 | 113,800 | $0.0353 | 1.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
