# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 11
**Date:** 2026-06-18
**Started:** 2026-06-18T15:10:58-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,463 |
| Total output tokens     | 11,236 |
| Total tokens            | 12,699 |
| Cache read tokens       | 1,372,342 |
| Cache creation tokens   | 53,146 |
| Total cost (USD)        | $0.7810 |
| Total API time          | 178.2s |
| API calls               | 20 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,387 | 18 | 0 | $0.0015 | 1.3s |
| 2 | claude-sonnet-4-6 | 3 | 352 | 20,621 | $0.0688 | 6.0s |
| 3 | claude-sonnet-4-6 | 56 | 323 | 35,908 | $0.0172 | 4.8s |
| 4 | claude-sonnet-4-6 | 1 | 5,845 | 45,423 | $0.1946 | 87.2s |
| 5 | claude-sonnet-4-6 | 1 | 710 | 70,310 | $0.0545 | 9.3s |
| 6 | claude-sonnet-4-6 | 1 | 217 | 76,378 | $0.0293 | 4.7s |
| 7 | claude-sonnet-4-6 | 1 | 198 | 77,212 | $0.0275 | 3.8s |
| 8 | claude-sonnet-4-6 | 1 | 312 | 77,576 | $0.0293 | 5.0s |
| 9 | claude-sonnet-4-6 | 1 | 385 | 77,946 | $0.0308 | 4.2s |
| 10 | claude-sonnet-4-6 | 1 | 328 | 78,377 | $0.0303 | 4.3s |
| 11 | claude-sonnet-4-6 | 1 | 271 | 78,862 | $0.0297 | 4.2s |
| 12 | claude-sonnet-4-6 | 1 | 266 | 79,389 | $0.0292 | 3.2s |
| 13 | claude-sonnet-4-6 | 1 | 257 | 79,760 | $0.0292 | 5.4s |
| 14 | claude-sonnet-4-6 | 1 | 402 | 80,126 | $0.0314 | 6.3s |
| 15 | claude-sonnet-4-6 | 1 | 507 | 80,985 | $0.0346 | 5.9s |
| 16 | claude-sonnet-4-6 | 1 | 175 | 81,714 | $0.0298 | 4.5s |
| 17 | claude-sonnet-4-6 | 1 | 172 | 82,420 | $0.0280 | 5.4s |
| 18 | claude-sonnet-4-6 | 1 | 168 | 82,613 | $0.0291 | 3.2s |
| 19 | claude-sonnet-4-6 | 1 | 122 | 83,273 | $0.0275 | 2.6s |
| 20 | claude-sonnet-4-6 | 1 | 208 | 83,449 | $0.0287 | 6.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
