# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 12
**Date:** 2026-06-18
**Started:** 2026-06-17T20:49:08-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 3,532 |
| Total output tokens     | 18,450 |
| Total tokens            | 21,982 |
| Cache read tokens       | 2,662,780 |
| Cache creation tokens   | 59,489 |
| Total cost (USD)        | $1.4402 |
| Total API time          | 317.2s |
| API calls               | 33 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,394 | 16 | 0 | $0.0015 | 1.5s |
| 2 | claude-sonnet-4-6 | 3 | 344 | 20,621 | $0.1005 | 5.4s |
| 3 | claude-sonnet-4-6 | 2,105 | 218 | 35,483 | $0.0225 | 4.9s |
| 4 | claude-sonnet-4-6 | 1 | 6,390 | 46,848 | $0.2487 | 107.0s |
| 5 | claude-sonnet-4-6 | 1 | 537 | 76,582 | $0.0357 | 10.2s |
| 6 | claude-sonnet-4-6 | 1 | 433 | 77,354 | $0.0337 | 6.7s |
| 7 | claude-sonnet-4-6 | 1 | 303 | 78,017 | $0.0313 | 4.4s |
| 8 | claude-sonnet-4-6 | 1 | 194 | 78,576 | $0.0289 | 3.9s |
| 9 | claude-sonnet-4-6 | 1 | 348 | 78,986 | $0.0313 | 6.5s |
| 10 | claude-sonnet-4-6 | 1 | 331 | 79,387 | $0.0321 | 5.5s |
| 11 | claude-sonnet-4-6 | 1 | 196 | 79,946 | $0.0295 | 8.7s |
| 12 | claude-sonnet-4-6 | 1 | 264 | 80,383 | $0.0301 | 5.3s |
| 13 | claude-sonnet-4-6 | 1 | 186 | 80,724 | $0.0311 | 3.4s |
| 14 | claude-sonnet-4-6 | 1 | 3,328 | 81,406 | $0.0770 | 59.8s |
| 15 | claude-sonnet-4-6 | 1 | 356 | 81,854 | $0.0506 | 4.5s |
| 16 | claude-sonnet-4-6 | 1 | 557 | 85,303 | $0.0373 | 7.4s |
| 17 | claude-sonnet-4-6 | 1 | 1,177 | 85,860 | $0.0474 | 12.1s |
| 18 | claude-sonnet-4-6 | 1 | 166 | 86,519 | $0.0361 | 2.8s |
| 19 | claude-sonnet-4-6 | 1 | 154 | 87,798 | $0.0324 | 2.6s |
| 20 | claude-sonnet-4-6 | 1 | 183 | 88,429 | $0.0346 | 2.9s |
| 21 | claude-sonnet-4-6 | 1 | 154 | 89,314 | $0.0357 | 2.9s |
| 22 | claude-sonnet-4-6 | 1 | 154 | 90,408 | $0.0361 | 2.6s |
| 23 | claude-sonnet-4-6 | 1 | 155 | 91,527 | $0.0431 | 4.1s |
| 24 | claude-sonnet-4-6 | 1 | 155 | 96,030 | $0.0325 | 2.6s |
| 25 | claude-sonnet-4-6 | 1 | 608 | 96,260 | $0.0429 | 7.9s |
| 26 | claude-sonnet-4-6 | 1 | 335 | 97,080 | $0.0384 | 5.4s |
| 27 | claude-sonnet-4-6 | 1 | 180 | 97,790 | $0.0347 | 3.0s |
| 28 | claude-sonnet-4-6 | 1 | 163 | 98,227 | $0.0331 | 2.7s |
| 29 | claude-sonnet-4-6 | 1 | 103 | 98,425 | $0.0344 | 2.7s |
| 30 | claude-sonnet-4-6 | 1 | 176 | 98,986 | $0.0331 | 2.9s |
| 31 | claude-sonnet-4-6 | 1 | 122 | 99,109 | $0.0336 | 2.6s |
| 32 | claude-sonnet-4-6 | 1 | 181 | 99,453 | $0.0351 | 4.8s |
| 33 | claude-sonnet-4-6 | 1 | 283 | 100,095 | $0.0349 | 7.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
