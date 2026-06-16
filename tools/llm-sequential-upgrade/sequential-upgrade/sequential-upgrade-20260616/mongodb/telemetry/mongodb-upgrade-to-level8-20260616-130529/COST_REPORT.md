# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 8
**Date:** 2026-06-16
**Started:** 2026-06-16T13:05:29-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,408 |
| Total output tokens     | 27,112 |
| Total tokens            | 29,520 |
| Cache read tokens       | 2,382,273 |
| Cache creation tokens   | 67,840 |
| Total cost (USD)        | $1.3781 |
| Total API time          | 345.2s |
| API calls               | 30 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,375 | 14 | 0 | $0.0024 | 1.2s |
| 2 | claude-sonnet-4-6 | 3 | 271 | 20,501 | $0.0584 | 5.9s |
| 3 | claude-sonnet-4-6 | 1 | 3,659 | 41,428 | $0.1187 | 51.1s |
| 4 | claude-sonnet-4-6 | 1 | 9,970 | 55,118 | $0.2181 | 117.6s |
| 5 | claude-sonnet-4-6 | 1 | 337 | 68,993 | $0.0636 | 6.5s |
| 6 | claude-sonnet-4-6 | 1 | 702 | 79,081 | $0.0363 | 7.2s |
| 7 | claude-sonnet-4-6 | 1 | 352 | 79,635 | $0.0322 | 5.6s |
| 8 | claude-sonnet-4-6 | 1 | 383 | 80,436 | $0.0316 | 4.5s |
| 9 | claude-sonnet-4-6 | 1 | 772 | 80,887 | $0.0377 | 7.8s |
| 10 | claude-sonnet-4-6 | 1 | 281 | 81,369 | $0.0319 | 3.8s |
| 11 | claude-sonnet-4-6 | 1 | 1,001 | 82,240 | $0.0411 | 10.4s |
| 12 | claude-sonnet-4-6 | 1 | 378 | 82,620 | $0.0350 | 4.9s |
| 13 | claude-sonnet-4-6 | 1 | 323 | 83,819 | $0.0318 | 5.7s |
| 14 | claude-sonnet-4-6 | 1 | 582 | 84,296 | $0.0356 | 7.0s |
| 15 | claude-sonnet-4-6 | 1 | 1,528 | 84,718 | $0.0509 | 15.0s |
| 16 | claude-sonnet-4-6 | 1 | 719 | 85,399 | $0.0425 | 8.6s |
| 17 | claude-sonnet-4-6 | 1 | 847 | 87,026 | $0.0423 | 10.3s |
| 18 | claude-sonnet-4-6 | 1 | 1,073 | 87,943 | $0.0460 | 10.4s |
| 19 | claude-sonnet-4-6 | 1 | 170 | 88,889 | $0.0336 | 3.2s |
| 20 | claude-sonnet-4-6 | 1 | 152 | 90,061 | $0.0313 | 3.2s |
| 21 | claude-sonnet-4-6 | 1 | 824 | 90,601 | $0.0407 | 9.1s |
| 22 | claude-sonnet-4-6 | 1 | 1,411 | 90,900 | $0.0519 | 15.1s |
| 23 | claude-sonnet-4-6 | 1 | 323 | 91,823 | $0.0384 | 5.3s |
| 24 | claude-sonnet-4-6 | 1 | 170 | 93,432 | $0.0321 | 2.8s |
| 25 | claude-sonnet-4-6 | 1 | 132 | 94,288 | $0.0312 | 2.4s |
| 26 | claude-sonnet-4-6 | 1 | 183 | 94,541 | $0.0322 | 4.7s |
| 27 | claude-sonnet-4-6 | 1 | 109 | 94,838 | $0.0314 | 4.1s |
| 28 | claude-sonnet-4-6 | 1 | 237 | 95,190 | $0.0340 | 3.8s |
| 29 | claude-sonnet-4-6 | 1 | 124 | 95,697 | $0.0331 | 3.1s |
| 30 | claude-sonnet-4-6 | 3 | 85 | 96,504 | $0.0322 | 4.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
