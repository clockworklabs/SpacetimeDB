# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-03T21:25:49-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 29 |
| Total output tokens     | 19,863 |
| Total tokens            | 19,892 |
| Cache read tokens       | 1,995,543 |
| Cache creation tokens   | 49,481 |
| Total cost (USD)        | $1.0822 |
| Total API time          | 327.4s |
| API calls               | 29 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 278 | 29,967 | $0.0354 | 8.3s |
| 2 | claude-sonnet-4-6 | 1 | 303 | 35,887 | $0.0226 | 3.8s |
| 3 | claude-sonnet-4-6 | 1 | 350 | 37,840 | $0.0317 | 5.6s |
| 4 | claude-sonnet-4-6 | 1 | 827 | 41,870 | $0.0434 | 16.2s |
| 5 | claude-sonnet-4-6 | 1 | 305 | 46,789 | $0.0392 | 5.3s |
| 6 | claude-sonnet-4-6 | 1 | 577 | 52,282 | $0.0392 | 14.4s |
| 7 | claude-sonnet-4-6 | 1 | 4,821 | 56,239 | $0.0968 | 71.6s |
| 8 | claude-sonnet-4-6 | 1 | 161 | 58,261 | $0.0484 | 4.7s |
| 9 | claude-sonnet-4-6 | 1 | 4,030 | 65,872 | $0.0906 | 66.3s |
| 10 | claude-sonnet-4-6 | 1 | 228 | 73,536 | $0.0286 | 4.5s |
| 11 | claude-sonnet-4-6 | 1 | 303 | 74,378 | $0.0279 | 5.5s |
| 12 | claude-sonnet-4-6 | 1 | 187 | 74,642 | $0.0268 | 3.4s |
| 13 | claude-sonnet-4-6 | 1 | 741 | 75,057 | $0.0353 | 8.6s |
| 14 | claude-sonnet-4-6 | 1 | 360 | 75,503 | $0.0313 | 4.6s |
| 15 | claude-sonnet-4-6 | 1 | 224 | 76,356 | $0.0280 | 4.6s |
| 16 | claude-sonnet-4-6 | 1 | 271 | 76,809 | $0.0281 | 4.1s |
| 17 | claude-sonnet-4-6 | 1 | 454 | 77,075 | $0.0313 | 5.3s |
| 18 | claude-sonnet-4-6 | 1 | 459 | 77,439 | $0.0322 | 5.5s |
| 19 | claude-sonnet-4-6 | 1 | 1,614 | 77,986 | $0.0497 | 18.3s |
| 20 | claude-sonnet-4-6 | 1 | 224 | 78,538 | $0.0333 | 8.2s |
| 21 | claude-sonnet-4-6 | 1 | 175 | 80,245 | $0.0277 | 4.2s |
| 22 | claude-sonnet-4-6 | 1 | 154 | 80,511 | $0.0272 | 9.1s |
| 23 | claude-sonnet-4-6 | 1 | 165 | 80,704 | $0.0273 | 5.3s |
| 24 | claude-sonnet-4-6 | 1 | 105 | 80,876 | $0.0275 | 2.2s |
| 25 | claude-sonnet-4-6 | 1 | 187 | 81,329 | $0.0276 | 4.0s |
| 26 | claude-sonnet-4-6 | 1 | 222 | 82,180 | $0.0286 | 3.1s |
| 27 | claude-sonnet-4-6 | 1 | 839 | 82,180 | $0.0388 | 17.3s |
| 28 | claude-sonnet-4-6 | 1 | 160 | 82,596 | $0.0307 | 3.1s |
| 29 | claude-sonnet-4-6 | 1 | 1,139 | 82,596 | $0.0471 | 9.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
