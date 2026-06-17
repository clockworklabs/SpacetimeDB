# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 3
**Date:** 2026-06-17
**Started:** 2026-06-17T11:10:59-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,272 |
| Total output tokens     | 19,552 |
| Total tokens            | 21,824 |
| Cache read tokens       | 2,257,962 |
| Cache creation tokens   | 48,126 |
| Total cost (USD)        | $1.2448 |
| Total API time          | 345.2s |
| API calls               | 34 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,237 | 22 | 0 | $0.0023 | 1.7s |
| 2 | claude-haiku-4-5-20251001 | 3 | 305 | 9,810 | $0.0056 | 4.4s |
| 3 | claude-sonnet-4-6 | 1 | 1,017 | 36,153 | $0.0485 | 14.6s |
| 4 | claude-sonnet-4-6 | 1 | 159 | 39,894 | $0.0451 | 4.5s |
| 5 | claude-sonnet-4-6 | 1 | 7,457 | 45,020 | $0.1773 | 154.6s |
| 6 | claude-sonnet-4-6 | 1 | 617 | 53,677 | $0.1006 | 12.8s |
| 7 | claude-sonnet-4-6 | 1 | 877 | 66,219 | $0.0375 | 11.1s |
| 8 | claude-sonnet-4-6 | 1 | 218 | 66,960 | $0.0294 | 3.4s |
| 9 | claude-sonnet-4-6 | 1 | 263 | 67,961 | $0.0264 | 3.5s |
| 10 | claude-sonnet-4-6 | 1 | 993 | 68,303 | $0.0376 | 12.3s |
| 11 | claude-sonnet-4-6 | 1 | 226 | 68,671 | $0.0306 | 3.7s |
| 12 | claude-sonnet-4-6 | 1 | 344 | 69,769 | $0.0286 | 4.9s |
| 13 | claude-sonnet-4-6 | 1 | 194 | 70,187 | $0.0268 | 2.4s |
| 14 | claude-sonnet-4-6 | 1 | 203 | 70,655 | $0.0260 | 4.5s |
| 15 | claude-sonnet-4-6 | 1 | 378 | 70,954 | $0.0294 | 6.2s |
| 16 | claude-sonnet-4-6 | 1 | 273 | 71,362 | $0.0288 | 4.3s |
| 17 | claude-sonnet-4-6 | 1 | 564 | 71,906 | $0.0323 | 7.4s |
| 18 | claude-sonnet-4-6 | 1 | 480 | 72,279 | $0.0335 | 5.5s |
| 19 | claude-sonnet-4-6 | 1 | 475 | 73,042 | $0.0325 | 5.4s |
| 20 | claude-sonnet-4-6 | 1 | 384 | 73,622 | $0.0313 | 6.7s |
| 21 | claude-sonnet-4-6 | 1 | 676 | 74,197 | $0.0353 | 8.3s |
| 22 | claude-sonnet-4-6 | 1 | 238 | 74,681 | $0.0306 | 4.8s |
| 23 | claude-sonnet-4-6 | 1 | 519 | 75,457 | $0.0330 | 6.1s |
| 24 | claude-sonnet-4-6 | 1 | 203 | 75,894 | $0.0295 | 4.9s |
| 25 | claude-sonnet-4-6 | 1 | 281 | 76,513 | $0.0314 | 5.1s |
| 26 | claude-sonnet-4-6 | 1 | 155 | 77,212 | $0.0283 | 3.6s |
| 27 | claude-sonnet-4-6 | 1 | 217 | 77,686 | $0.0293 | 2.9s |
| 28 | claude-sonnet-4-6 | 1 | 234 | 78,146 | $0.0290 | 4.3s |
| 29 | claude-sonnet-4-6 | 1 | 238 | 78,493 | $0.0329 | 4.6s |
| 30 | claude-sonnet-4-6 | 1 | 360 | 79,463 | $0.0343 | 6.6s |
| 31 | claude-sonnet-4-6 | 1 | 170 | 80,308 | $0.0289 | 2.5s |
| 32 | claude-sonnet-4-6 | 1 | 299 | 80,687 | $0.0315 | 5.3s |
| 33 | claude-sonnet-4-6 | 1 | 159 | 81,157 | $0.0295 | 2.5s |
| 34 | claude-sonnet-4-6 | 1 | 354 | 81,624 | $0.0309 | 9.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
