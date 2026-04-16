# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-04-07
**Started:** 2026-04-07T11:35:11-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 39 |
| Total output tokens     | 15,374 |
| Total tokens            | 15,413 |
| Cache read tokens       | 2,806,834 |
| Cache creation tokens   | 48,395 |
| Total cost (USD)        | $1.2543 |
| Total API time          | 258.4s |
| API calls               | 37 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 207 | 20,510 | $0.0546 | 7.3s |
| 2 | claude-sonnet-4-6 | 1 | 319 | 47,528 | $0.0304 | 9.1s |
| 3 | claude-sonnet-4-6 | 1 | 160 | 50,543 | $0.0342 | 4.1s |
| 4 | claude-sonnet-4-6 | 1 | 160 | 57,865 | $0.0365 | 4.1s |
| 5 | claude-sonnet-4-6 | 1 | 1,513 | 62,318 | $0.0512 | 26.5s |
| 6 | claude-sonnet-4-6 | 1 | 3,248 | 64,923 | $0.0767 | 48.4s |
| 7 | claude-sonnet-4-6 | 1 | 227 | 67,194 | $0.0358 | 5.2s |
| 8 | claude-sonnet-4-6 | 1 | 227 | 70,737 | $0.0264 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 650 | 70,737 | $0.0337 | 7.9s |
| 10 | claude-sonnet-4-6 | 1 | 227 | 72,231 | $0.0282 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 186 | 74,530 | $0.0287 | 6.3s |
| 12 | claude-sonnet-4-6 | 1 | 227 | 75,475 | $0.0280 | 4.3s |
| 13 | claude-sonnet-4-6 | 1 | 764 | 75,991 | $0.0353 | 10.8s |
| 14 | claude-sonnet-4-6 | 1 | 367 | 76,260 | $0.0317 | 4.7s |
| 15 | claude-sonnet-4-6 | 1 | 597 | 77,135 | $0.0338 | 7.5s |
| 16 | claude-sonnet-4-6 | 1 | 281 | 77,594 | $0.0301 | 4.2s |
| 17 | claude-sonnet-4-6 | 1 | 274 | 78,283 | $0.0290 | 3.9s |
| 18 | claude-sonnet-4-6 | 1 | 530 | 78,656 | $0.0329 | 6.6s |
| 19 | claude-sonnet-4-6 | 1 | 631 | 79,022 | $0.0363 | 7.6s |
| 20 | claude-sonnet-4-6 | 1 | 331 | 79,849 | $0.0316 | 6.4s |
| 21 | claude-sonnet-4-6 | 1 | 181 | 80,572 | $0.0285 | 3.7s |
| 22 | claude-sonnet-4-6 | 1 | 160 | 81,525 | $0.0278 | 2.2s |
| 23 | claude-sonnet-4-6 | 1 | 233 | 82,323 | $0.0294 | 4.5s |
| 24 | claude-sonnet-4-6 | 1 | 182 | 82,637 | $0.0286 | 3.5s |
| 25 | claude-sonnet-4-6 | 1 | 522 | 82,912 | $0.0336 | 11.8s |
| 26 | claude-sonnet-4-6 | 1 | 566 | 83,149 | $0.0381 | 9.9s |
| 27 | claude-sonnet-4-6 | 1 | 160 | 84,401 | $0.0302 | 2.5s |
| 28 | claude-sonnet-4-6 | 1 | 160 | 84,401 | $0.0323 | 3.1s |
| 29 | claude-sonnet-4-6 | 1 | 591 | 85,624 | $0.0365 | 5.3s |
| 30 | claude-sonnet-4-6 | 1 | 182 | 86,151 | $0.0311 | 6.3s |
| 31 | claude-sonnet-4-6 | 1 | 181 | 86,834 | $0.0295 | 3.9s |
| 32 | claude-sonnet-4-6 | 1 | 237 | 87,034 | $0.0322 | 3.6s |
| 33 | claude-sonnet-4-6 | 1 | 122 | 87,720 | $0.0296 | 3.1s |
| 34 | claude-sonnet-4-6 | 1 | 170 | 88,108 | $0.0295 | 3.0s |
| 35 | claude-sonnet-4-6 | 1 | 196 | 88,108 | $0.0313 | 3.6s |
| 36 | claude-sonnet-4-6 | 1 | 170 | 88,611 | $0.0300 | 3.4s |
| 37 | claude-sonnet-4-6 | 1 | 235 | 89,343 | $0.0311 | 3.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
