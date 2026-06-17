# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 12
**Date:** 2026-06-17
**Started:** 2026-06-17T14:21:06-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,842 |
| Total output tokens     | 33,123 |
| Total tokens            | 35,965 |
| Cache read tokens       | 3,133,897 |
| Cache creation tokens   | 87,405 |
| Total cost (USD)        | $1.9642 |
| Total API time          | 509.3s |
| API calls               | 35 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,806 | 14 | 0 | $0.0029 | 1.0s |
| 2 | claude-sonnet-4-6 | 3 | 461 | 20,621 | $0.1101 | 8.1s |
| 3 | claude-sonnet-4-6 | 1 | 207 | 36,788 | $0.0221 | 2.9s |
| 4 | claude-sonnet-4-6 | 1 | 224 | 38,111 | $0.0381 | 6.8s |
| 5 | claude-sonnet-4-6 | 1 | 8,615 | 41,998 | $0.3142 | 143.1s |
| 6 | claude-sonnet-4-6 | 1 | 12,459 | 70,720 | $0.2709 | 179.1s |
| 7 | claude-sonnet-4-6 | 1 | 766 | 81,186 | $0.1113 | 10.6s |
| 8 | claude-sonnet-4-6 | 1 | 590 | 93,769 | $0.0423 | 6.9s |
| 9 | claude-sonnet-4-6 | 1 | 441 | 94,659 | $0.0392 | 6.0s |
| 10 | claude-sonnet-4-6 | 1 | 315 | 95,354 | $0.0366 | 5.7s |
| 11 | claude-sonnet-4-6 | 1 | 315 | 95,900 | $0.0360 | 3.9s |
| 12 | claude-sonnet-4-6 | 1 | 313 | 96,320 | $0.0361 | 3.9s |
| 13 | claude-sonnet-4-6 | 1 | 403 | 96,740 | $0.0376 | 5.2s |
| 14 | claude-sonnet-4-6 | 1 | 381 | 97,158 | $0.0385 | 4.7s |
| 15 | claude-sonnet-4-6 | 1 | 343 | 97,765 | $0.0374 | 4.2s |
| 16 | claude-sonnet-4-6 | 1 | 373 | 98,251 | $0.0378 | 5.2s |
| 17 | claude-sonnet-4-6 | 1 | 381 | 98,699 | $0.0382 | 5.5s |
| 18 | claude-sonnet-4-6 | 1 | 387 | 99,177 | $0.0385 | 4.9s |
| 19 | claude-sonnet-4-6 | 1 | 208 | 99,663 | $0.0360 | 9.3s |
| 20 | claude-sonnet-4-6 | 1 | 653 | 100,155 | $0.0418 | 10.7s |
| 21 | claude-sonnet-4-6 | 1 | 194 | 100,475 | $0.0376 | 3.2s |
| 22 | claude-sonnet-4-6 | 1 | 262 | 101,234 | $0.0369 | 5.0s |
| 23 | claude-sonnet-4-6 | 1 | 1,107 | 102,344 | $0.0492 | 14.0s |
| 24 | claude-sonnet-4-6 | 1 | 379 | 102,664 | $0.0437 | 5.1s |
| 25 | claude-sonnet-4-6 | 1 | 258 | 103,871 | $0.0379 | 4.4s |
| 26 | claude-sonnet-4-6 | 1 | 344 | 104,350 | $0.0392 | 4.8s |
| 27 | claude-sonnet-4-6 | 1 | 712 | 104,807 | $0.0448 | 9.4s |
| 28 | claude-sonnet-4-6 | 1 | 306 | 105,251 | $0.0410 | 4.6s |
| 29 | claude-sonnet-4-6 | 1 | 349 | 106,063 | $0.0395 | 4.6s |
| 30 | claude-sonnet-4-6 | 1 | 452 | 106,469 | $0.0414 | 6.1s |
| 31 | claude-sonnet-4-6 | 1 | 175 | 107,788 | $0.0388 | 3.6s |
| 32 | claude-sonnet-4-6 | 1 | 158 | 108,424 | $0.0361 | 2.5s |
| 33 | claude-sonnet-4-6 | 1 | 189 | 108,617 | $0.0382 | 3.5s |
| 34 | claude-sonnet-4-6 | 1 | 122 | 109,075 | $0.0367 | 2.5s |
| 35 | claude-sonnet-4-6 | 1 | 267 | 109,431 | $0.0376 | 8.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
