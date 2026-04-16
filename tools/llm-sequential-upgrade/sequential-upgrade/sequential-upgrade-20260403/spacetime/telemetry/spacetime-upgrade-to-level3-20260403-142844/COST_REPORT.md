# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 3
**Date:** 2026-04-03
**Started:** 2026-04-03T14:28:44-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 32 |
| Total output tokens     | 21,639 |
| Total tokens            | 21,671 |
| Cache read tokens       | 2,636,508 |
| Cache creation tokens   | 46,533 |
| Total cost (USD)        | $1.2901 |
| Total API time          | 338.8s |
| API calls               | 32 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 278 | 39,400 | $0.0508 | 5.2s |
| 2 | claude-sonnet-4-6 | 1 | 921 | 53,200 | $0.0612 | 18.8s |
| 3 | claude-sonnet-4-6 | 1 | 295 | 61,586 | $0.0429 | 6.3s |
| 4 | claude-sonnet-4-6 | 1 | 303 | 66,921 | $0.0281 | 4.2s |
| 5 | claude-sonnet-4-6 | 1 | 5,138 | 67,845 | $0.1057 | 78.1s |
| 6 | claude-sonnet-4-6 | 1 | 3,360 | 70,044 | $0.0939 | 54.9s |
| 7 | claude-sonnet-4-6 | 1 | 871 | 79,434 | $0.0387 | 12.1s |
| 8 | claude-sonnet-4-6 | 1 | 303 | 79,924 | $0.0322 | 4.2s |
| 9 | claude-sonnet-4-6 | 1 | 322 | 80,913 | $0.0304 | 4.8s |
| 10 | claude-sonnet-4-6 | 1 | 272 | 81,258 | $0.0301 | 6.8s |
| 11 | claude-sonnet-4-6 | 1 | 828 | 81,698 | $0.0383 | 10.8s |
| 12 | claude-sonnet-4-6 | 1 | 303 | 82,069 | $0.0326 | 5.7s |
| 13 | claude-sonnet-4-6 | 1 | 371 | 82,996 | $0.0318 | 6.3s |
| 14 | claude-sonnet-4-6 | 1 | 356 | 83,341 | $0.0321 | 4.5s |
| 15 | claude-sonnet-4-6 | 1 | 311 | 83,813 | $0.0315 | 5.9s |
| 16 | claude-sonnet-4-6 | 1 | 312 | 84,268 | $0.0315 | 4.3s |
| 17 | claude-sonnet-4-6 | 1 | 312 | 84,685 | $0.0316 | 4.0s |
| 18 | claude-sonnet-4-6 | 1 | 312 | 85,096 | $0.0318 | 4.0s |
| 19 | claude-sonnet-4-6 | 1 | 295 | 85,507 | $0.0316 | 3.8s |
| 20 | claude-sonnet-4-6 | 1 | 303 | 85,921 | $0.0328 | 4.4s |
| 21 | claude-sonnet-4-6 | 1 | 820 | 86,593 | $0.0396 | 9.5s |
| 22 | claude-sonnet-4-6 | 1 | 508 | 86,938 | $0.0371 | 7.4s |
| 23 | claude-sonnet-4-6 | 1 | 707 | 87,852 | $0.0392 | 11.3s |
| 24 | claude-sonnet-4-6 | 1 | 576 | 88,454 | $0.0382 | 7.9s |
| 25 | claude-sonnet-4-6 | 1 | 1,023 | 89,255 | $0.0446 | 13.4s |
| 26 | claude-sonnet-4-6 | 1 | 173 | 89,925 | $0.0338 | 3.7s |
| 27 | claude-sonnet-4-6 | 1 | 303 | 92,452 | $0.0335 | 4.0s |
| 28 | claude-sonnet-4-6 | 1 | 849 | 93,122 | $0.0423 | 13.9s |
| 29 | claude-sonnet-4-6 | 1 | 176 | 99,837 | $0.0348 | 3.6s |
| 30 | claude-sonnet-4-6 | 1 | 184 | 100,429 | $0.0336 | 5.6s |
| 31 | claude-sonnet-4-6 | 1 | 253 | 100,623 | $0.0358 | 4.5s |
| 32 | claude-sonnet-4-6 | 1 | 301 | 101,109 | $0.0377 | 4.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
